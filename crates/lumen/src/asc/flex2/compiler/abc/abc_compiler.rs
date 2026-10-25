//! Port of `flex2.compiler.abc.AbcCompiler`: a library script's ABC, imported into the model by `AbcParser` and taken
//! through the same phases for its type information. Its bytecode is the library's, unchanged.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::asc::abc::abc_parser::AbcParser;
use crate::asc::emit::global_builder::new_global_builder;
use crate::asc::flex2::compiler::as3::as3_compiler::{
    clean_node_factory, clean_slots, evaluate_loader_class_base, inherit_slots, new_unit_context, program_frame, program_state,
    register_classes, SubCompiler,
};
use crate::asc::flex2::compiler::as3::evaluator_adapter;
use crate::asc::flex2::compiler::as3::syntax_tree_evaluator::SyntaxTreeEvaluator;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::source::{Source, SourceRef};
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::parser::evaluator::evaluate;
use crate::asc::parser::node::ProgramNode;
use crate::asc::semantics::constant_evaluator::ConstantEvaluator;
use crate::asc::semantics::flow_analyzer::FlowAnalyzer;
use crate::asc::semantics::flow_graph_emitter::FlowGraphEmitter;
use crate::asc::semantics::object_value::ObjectValue;
use crate::asc::util::context::CompilerHandler;

/// The handler `AbcCompiler.parse1` installs: messages name the script, without positions.
struct AbcHandler {
    path: String,
}

impl AbcHandler {
    fn name<'a>(&'a self, filename: &'a str) -> &'a str {
        if filename.is_empty() {
            &self.path
        } else {
            filename
        }
    }
}

impl CompilerHandler for AbcHandler {
    fn error(&mut self, filename: &str, _ln: i32, _col: i32, msg: &str, _source: &str, code: i32) {
        toolkit::log_path(Level::Error, self.name(filename), msg, code);
    }
    fn warning(&mut self, filename: &str, _ln: i32, _col: i32, msg: &str, _source: &str, code: i32) {
        toolkit::log_path(Level::Warning, self.name(filename), msg, code);
    }
    fn error_no_code(&mut self, filename: &str, _ln: i32, _col: i32, msg: &str, _source: &str) {
        toolkit::log_path(Level::Error, self.name(filename), msg, -1);
    }
    fn warning_no_code(&mut self, filename: &str, _ln: i32, _col: i32, msg: &str, _source: &str) {
        toolkit::log_path(Level::Warning, self.name(filename), msg, -1);
    }
}

#[derive(Default)]
pub struct AbcCompiler;

impl SubCompiler for AbcCompiler {
    fn get_name(&self) -> &'static str {
        "abc"
    }

    fn is_supported(&self, mime_type: &str) -> bool {
        mime_type == mime_mappings::ABC
    }

    fn parse1(&mut self, source: &SourceRef, symbol_table: &mut SymbolTable) -> Option<UnitRef> {
        let existing = source.borrow().get_compilation_unit();
        if let Some(u) = &existing {
            let u = u.borrow();
            if u.has_type_info || u.get_syntax_tree().is_some() {
                drop(u);
                return existing;
            }
        }
        let path = source.borrow().get_name().to_string();
        let cx = new_unit_context(source, symbol_table, Rc::new(RefCell::new(AbcHandler { path: path.clone() })));
        let abc = match source.borrow().to_byte_array() {
            Ok(b) => b,
            Err(e) => {
                toolkit::log_path(Level::Error, &path, &e.to_string(), -1);
                return None;
            }
        };
        let mut node = None;
        if abc.is_empty() {
            toolkit::log_for(CompilerMessage::error("No bytecode is available."), &path);
        } else {
            node = AbcParser::new(&cx, abc.clone()).parse_abc();
            if node.is_none() && toolkit::error_count() == 0 {
                toolkit::log_for(CompilerMessage::error("abc bytecode decoding failed."), &path);
            }
            clean_node_factory(&cx);
        }
        if toolkit::error_count() > 0 {
            return None;
        }
        let node = node?;
        let unit = match existing {
            Some(u) => {
                u.borrow_mut().set_syntax_tree(Some(node.clone()));
                u
            }
            None => Source::new_compilation_unit(source, Some(node.clone())),
        };
        {
            let mut u = unit.borrow_mut();
            u.context.set_asc_context(Some(cx.clone()));
            u.bytes = abc;
        }
        let mut tree_evaluator = SyntaxTreeEvaluator::new(unit.clone());
        evaluator_adapter::evaluate(&mut tree_evaluator, &cx, &node);
        crate::asc::flex2::compiler::as3::style_evaluator::style_extension_parse1(&unit);
        Some(unit)
    }

    fn parse2(&mut self, _unit: &UnitRef, _symbol_table: &mut SymbolTable) {}

    fn analyze1(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        if program_state(&node) != ProgramNode::Inheritance {
            return;
        }
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        let global = ObjectValue::with_builder(&cx.borrow(), new_global_builder(), None);
        cx.borrow().push_scope(global);
        let name = unit.borrow().get_source().borrow().get_name().to_string();
        let flowem = Rc::new(RefCell::new(FlowGraphEmitter::new(&cx, &name, false)));
        let mut flower = FlowAnalyzer::new(flowem);
        evaluate(&node, &cx, &mut flower);
        cx.borrow().pop_scope();
        unit.borrow_mut().context.flow_analyzer = Some(flower);
        if toolkit::error_count() > 0 {
            return;
        }
        unit.borrow_mut().type_info = node.borrow().program().unwrap().frame.clone();
    }

    fn analyze2(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        if program_state(&node) != ProgramNode::Else {
            return;
        }
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        let mut flower = unit.borrow_mut().context.flow_analyzer.take().expect("NullPointerException");
        unit.borrow_mut().context.processed = Some(HashSet::with_capacity(15));
        inherit_slots(unit, |u| &u.inheritance, symbol_table);
        inherit_slots(unit, |u| &u.namespaces, symbol_table);
        cx.borrow().push_scope(program_frame(&node));
        evaluate(&node, &cx, &mut flower);
        cx.borrow().pop_scope();
        unit.borrow_mut().context.flow_analyzer = Some(flower);
    }

    fn analyze3(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        inherit_slots(unit, |u| &u.types, symbol_table);
        inherit_slots(unit, |u| &u.namespaces, symbol_table);
        cx.borrow().push_scope(program_frame(&node));
        let mut analyzer = ConstantEvaluator::new(&cx);
        analyzer.preprocess_definition_type_info_program(&cx, &node);
        cx.borrow().pop_scope();
        unit.borrow_mut().context.constant_evaluator = Some(analyzer);
    }

    fn analyze4(&mut self, unit: &UnitRef, symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            crate::asc::flex2::compiler::as3::style_evaluator::register_unit_styles(unit, symbol_table);
            evaluate_loader_class_base(unit, symbol_table);
            return;
        }
        let node = unit.borrow().get_syntax_tree().unwrap();
        let cx = unit.borrow().context.get_asc_context().unwrap();
        symbol_table.per_compile_data.borrow_mut().handler = cx.borrow().get_handler();
        cx.borrow().push_scope(program_frame(&node));
        let mut analyzer = unit.borrow_mut().context.constant_evaluator.take().expect("NullPointerException");
        evaluate(&node, &cx, &mut analyzer);
        cx.borrow().pop_scope();
        if toolkit::error_count() > 0 {
            return;
        }
        register_classes(unit, &node, symbol_table);
        crate::asc::flex2::compiler::as3::style_evaluator::register_unit_styles(unit, symbol_table);
        evaluate_loader_class_base(unit, symbol_table);
    }

    fn generate(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        if unit.borrow().has_type_info {
            return;
        }
        let cx = unit.borrow_mut().context.remove_asc_context();
        let (type_info, defs) = {
            let u = unit.borrow();
            (u.type_info.clone(), u.top_level_definitions.clone())
        };
        clean_slots(type_info.as_ref(), cx.as_ref(), &defs);
        if let Some(cx) = cx {
            cx.borrow_mut().handler = None;
        }
    }
}
