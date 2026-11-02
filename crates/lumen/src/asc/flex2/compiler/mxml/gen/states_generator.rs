//! Port of `flex2.compiler.mxml.gen.StatesGenerator`: the states initialization the document constructor runs.

use std::collections::HashSet;
use std::rc::Rc;

use super::code_fragment_list::CodeFragmentList;
use crate::asc::flex2::compiler::mxml::rep::deps::StandardDefs;
use crate::asc::flex2::compiler::mxml::rep::states_model::{OverrideRef, StatesModel};
use crate::asc::flex2::compiler::util::name_formatter;
use crate::asc::flex2::compiler::as3::abstract_syntax_tree_util::{self as ast, Nf};
use crate::asc::parser::node::NodeRef;
use crate::asc::parser::tokens::LEFTBRACKET_TOKEN;

pub const INDENT: &str = "  ";

pub struct StatesGenerator {
    bindings_queue: Vec<OverrideRef>,
    standard_defs: Rc<StandardDefs>,
}

impl StatesGenerator {
    pub fn new(defs: Rc<StandardDefs>) -> StatesGenerator {
        StatesGenerator { bindings_queue: Vec::new(), standard_defs: defs }
    }

    pub fn get_states_initializer_fragments(&mut self, model: &StatesModel) -> CodeFragmentList {
        let mut list = CodeFragmentList::new();
        let indent = "\t\t";
        self.gen_shared_factories(model, &mut list, indent);
        list.add("\n", 0);
        self.gen_states(model, &mut list, indent);
        list.add("\n", 0);
        self.gen_binding_initializers(&mut list, indent);
        list.add("\n", 0);
        self.gen_state_initializers(model, &mut list, indent);
        list.add("\n", 0);
        self.gen_immediate_inits(model, &mut list, indent);
        list
    }

    /// `getStatesASTInitializers(model, nodeFactory, configNamespaces, generateDocComments, statementList)`.
    pub fn get_states_ast_initializers(&mut self, model: &StatesModel, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let results = self.gen_shared_factories_ast(model, nf, statement_list);
        let results = self.gen_states_ast(model, nf, config_namespaces, generate_doc_comments, results);
        let results = self.gen_binding_initializers_ast(nf, results);
        let results = self.gen_state_initializers_ast(model, nf, config_namespaces, generate_doc_comments, results);
        self.gen_immediate_inits_ast(model, nf, results)
    }

    fn gen_shared_factories(&self, model: &StatesModel, list: &mut CodeFragmentList, indent: &str) {
        let shared: Vec<_> = model.shared_objects.borrow().values().cloned().collect();
        let mut indent = indent.to_string();
        for symbol in shared {
            let lval = format!("{indent}var {}_factory:{} = \n", symbol.name, name_formatter::retrieve_class_name(&self.standard_defs.class_deferredinstancefromfunction));
            indent.push_str(INDENT);
            let suffix = if symbol.model.is_declared() { "_i" } else { "_c" };
            let is_transient = symbol.model.get_is_transient();
            let rval = format!(
                "{indent}new {}({}{suffix}{});",
                name_formatter::to_dot(&self.standard_defs.class_deferredinstancefromfunction),
                symbol.name,
                if is_transient { format!(", {}_r", symbol.name) } else { String::new() }
            );
            indent.truncate(indent.len() - INDENT.len());
            list.add2(&lval, &rval, 0);
        }
    }

    fn gen_shared_factories_ast(&self, model: &StatesModel, nf: &Nf, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let shared: Vec<_> = model.shared_objects.borrow().values().cloned().collect();
        let mut result = statement_list;
        for symbol in shared {
            let var_name = format!("{}_factory", symbol.name);
            let type_name = name_formatter::retrieve_class_name("DeferredInstanceFromFunction");
            let factory = format!("{}{}", symbol.name, if symbol.model.is_declared() { "_i" } else { "_c" });
            let reset_func = format!("{}_r", symbol.name);
            let member_expression = ast::generate_getter_selector(&mut nf.f(), &factory, true);
            let mut call_expression_argument_list = nf.f().argument_list(None, Some(member_expression));
            if symbol.model.get_is_transient() {
                let member_expression = ast::generate_getter_selector(&mut nf.f(), &reset_func, true);
                call_expression_argument_list = nf.f().argument_list(Some(call_expression_argument_list), Some(member_expression));
            }
            let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), self.standard_defs.get_core_package(), &type_name, false);
            let call_expression = ast::call(nf, qualified_identifier, Some(call_expression_argument_list), true);
            let ad = nf.f().member_expression(None, Some(call_expression));
            let variable_definition = ast::generate_variable(&mut nf.f(), &var_name, &type_name, false, Some(ad));
            result = Some(nf.f().statement_list(result, Some(variable_definition)));
        }
        result
    }

    fn gen_binding_initializers(&self, list: &mut CodeFragmentList, indent: &str) {
        for symbol in &self.bindings_queue {
            let decl = symbol.declaration.borrow().clone().unwrap_or_else(|| "null".into());
            list.add3(indent, &name_formatter::to_dot(&self.standard_defs.class_bindingmanager), &format!(".executeBindings(this, \"{decl}\", {decl});"), 0);
        }
    }

    fn gen_binding_initializers_ast(&self, nf: &Nf, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = statement_list;
        for symbol in &self.bindings_queue {
            let qualified_identifier = ast::generate_qualified_identifier(&mut nf.f(), self.standard_defs.get_binding_package(), "BindingManager", false);
            let bind_expression = nf.f().get_expression(Some(qualified_identifier));
            let lvalue = nf.f().member_expression(None, Some(bind_expression));
            let this_expression = nf.f().this_expression(0);
            let exec_args = nf.f().argument_list(None, Some(this_expression));
            let decl = symbol.declaration.borrow().clone().expect("NullPointerException");
            let literal_string = nf.f().literal_string_intern(&decl, false);
            let exec_args = nf.f().argument_list(Some(exec_args), Some(literal_string));
            let rval_identifier = nf.f().identifier_intern(&decl, false);
            let get_expression = nf.f().get_expression(Some(rval_identifier));
            let rvalue = nf.f().member_expression(None, Some(get_expression));
            let exec_args = nf.f().argument_list(Some(exec_args), Some(rvalue));
            let bind_identifier = nf.f().identifier_intern("executeBindings", false);
            let selector = ast::call(nf, bind_identifier, Some(exec_args), false);
            let expression_statement = ast::expr_stmt(nf, Some(lvalue), selector);
            result = Some(nf.f().statement_list(result, Some(expression_statement)));
        }
        result
    }

    fn gen_immediate_inits(&self, model: &StatesModel, list: &mut CodeFragmentList, indent: &str) {
        let objects = model.early_init_objects.borrow().clone();
        let mut indent = indent.to_string();
        for symbol in objects {
            let lval = format!("{indent}{symbol}_factory.getInstance();\n");
            indent.push_str(INDENT);
            list.add(&lval, 0);
        }
    }

    fn gen_immediate_inits_ast(&self, model: &StatesModel, nf: &Nf, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = statement_list;
        let objects = model.early_init_objects.borrow().clone();
        for symbol in objects {
            let id_node = nf.f().identifier_intern(&format!("{symbol}_factory"), false);
            let get_index_expression = nf.f().get_expression(Some(id_node));
            let base = nf.f().member_expression(None, Some(get_index_expression));
            let get_node = nf.f().identifier_intern("getInstance", false);
            let selector = ast::call(nf, get_node, None, false);
            let expression_statement = ast::expr_stmt(nf, Some(base), selector);
            result = Some(nf.f().statement_list(result, Some(expression_statement)));
        }
        result
    }

    fn gen_states(&mut self, model: &StatesModel, list: &mut CodeFragmentList, indent: &str) {
        let states = model.info.get_state_names();
        if !states.is_empty() {
            let mut indent = indent.to_string();
            list.add2(&indent, "states = [", 0);
            indent.push_str(INDENT);
            for (i, name) in states.iter().enumerate() {
                let state = model.state_by_name(name);
                state.get_definition_body(model, list, &indent, &mut self.bindings_queue);
                if i + 1 < states.len() {
                    list.add2(&indent, ",", 0);
                }
            }
            indent.truncate(indent.len() - INDENT.len());
            list.add2(&indent, "];", 0);
        }
    }

    fn gen_states_ast(&mut self, model: &StatesModel, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = statement_list;
        let states = model.info.get_state_names();
        if !states.is_empty() {
            let mut states_argument_list: Option<NodeRef> = None;
            for name in states.iter() {
                let state = model.state_by_name(name);
                let state_expression = state.generate_definition_body(model, nf, config_namespaces, generate_doc_comments, &mut self.bindings_queue);
                states_argument_list = Some(nf.f().argument_list(states_argument_list, Some(state_expression)));
            }
            let literal_array = nf.f().literal_array(states_argument_list);
            let arg_list = nf.f().argument_list(None, Some(literal_array));
            let states_identifier = nf.f().identifier_intern("states", false);
            let selector = nf.f().set_expression(Some(states_identifier), Some(arg_list), false);
            let expression_statement = ast::expr_stmt(nf, None, selector);
            result = Some(nf.f().statement_list(result, Some(expression_statement)));
        }
        result
    }

    fn gen_state_initializers(&self, model: &StatesModel, list: &mut CodeFragmentList, indent: &str) {
        for (count, name) in model.info.get_state_names().iter().enumerate() {
            let state = model.state_by_name(name);
            if state.is_declared() {
                list.add2(indent, &format!("{}= states[{count}];", state.get_id().unwrap_or_default()), 0);
            }
            for ei in state.get_events() {
                list.add2(indent, &format!("states[{count}].addEventListener(\"{}\", {} );", ei.get_name(), ei.get_value_expr()), 0);
            }
        }
    }

    fn gen_state_initializers_ast(&self, model: &StatesModel, nf: &Nf, config_namespaces: &HashSet<String>, generate_doc_comments: bool, statement_list: Option<NodeRef>) -> Option<NodeRef> {
        let mut result = statement_list;
        for (count, name) in model.info.get_state_names().iter().enumerate() {
            let state = model.state_by_name(name);
            let identifier = state.get_id().expect("NullPointerException");
            let state_identifier = nf.f().identifier_intern(&identifier, false);
            let states_identifier = nf.f().identifier_intern("states", false);
            let number_node = nf.f().literal_number_int(count as i32);
            let get_index_arg_list = nf.f().argument_list(None, Some(number_node));
            let get_index_expression = nf.f().get_expression(Some(get_index_arg_list));
            ast::set_mode(&get_index_expression, LEFTBRACKET_TOKEN);
            let get_states_expression = nf.f().get_expression(Some(states_identifier));
            let base = nf.f().member_expression(None, Some(get_states_expression));
            let get_expr = nf.f().member_expression(Some(base), Some(get_index_expression));
            if state.is_declared() {
                let arg_list = nf.f().argument_list(None, Some(get_expr.clone()));
                let selector = nf.f().set_expression(Some(state_identifier), Some(arg_list), false);
                let expression_statement = ast::expr_stmt(nf, None, selector);
                result = Some(nf.f().statement_list(result, Some(expression_statement)));
            }
            for ei in state.get_events() {
                let add_event_identifier = nf.f().identifier_intern("addEventListener", false);
                let event_name = nf.f().literal_string(&ei.get_name());
                let value_node = ei.generate_value_expr(nf, config_namespaces, generate_doc_comments);
                let add_event_args = nf.f().argument_list(None, Some(event_name));
                let add_event_args = nf.f().argument_list(Some(add_event_args), Some(value_node));
                let add_listener = ast::call(nf, add_event_identifier, Some(add_event_args), false);
                let expression_statement = ast::expr_stmt(nf, Some(get_expr.clone()), add_listener);
                result = Some(nf.f().statement_list(result, Some(expression_statement)));
            }
        }
        result
    }
}
