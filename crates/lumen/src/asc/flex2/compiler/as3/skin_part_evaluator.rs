//! Port of `flex2.compiler.as3.SkinPartEvaluator`: adds `_skinParts` and the `skinParts` getter to classes with skin
//! parts, own or inherited.

use super::abstract_syntax_tree_util as ast;
use super::evaluator_adapter::{walk, Adapter};
use super::reflect::node_magic;
use crate::asc::flex2::compiler::symbol_table::SymbolTable;
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::parser::tokens::GET_TOKEN;
use crate::asc::util::context::{Context, ContextRef};
use crate::asc::util::jhash::JHashMap;

const SKINPARTS: &str = "skinParts";
const _SKINPARTS: &str = "_skinParts";
const OVERRIDE: &str = "override";
const OBJECT: &str = "Object";

pub struct SkinPartEvaluator<'a> {
    symbol_table: &'a SymbolTable,
    current_context: Option<ContextRef>,
}

impl<'a> SkinPartEvaluator<'a> {
    pub fn new(symbol_table: &'a SymbolTable) -> SkinPartEvaluator<'a> {
        SkinPartEvaluator { symbol_table, current_context: None }
    }

    fn class_definition(&mut self, context: &ContextRef, node: &NodeRef) {
        walk(self, context, node);
        let type_analyzer = self.symbol_table.get_type_analyzer();
        let class_name = node_magic::get_class_name(node);
        self.current_context = Some(context.clone());
        type_analyzer.borrow_mut().evaluate_class_definition(context, node, self.symbol_table);
        let class_info = type_analyzer.borrow().get_class_info(&class_name).expect("NullPointerException");
        let parts = class_info.borrow().get_skin_parts(true);
        if let Some(parts) = parts.filter(|p| !p.is_empty()) {
            let statements = node.borrow().class_definition().unwrap().statements.clone();
            let result = self.gen_skin_parts_ast(&parts, statements);
            node.borrow_mut().class_definition_mut().unwrap().statements = Some(result);
        }
    }

    fn gen_skin_parts_ast(&self, parts: &JHashMap<String, bool>, statements: Option<NodeRef>) -> NodeRef {
        let cx = self.current_context.clone().unwrap();
        let nf = Context::get_node_factory(&cx);
        let mut parts_list: Option<NodeRef> = None;
        let (type_expression, literal_object) = {
            let mut node_factory = nf.borrow_mut();
            for (key, required) in parts.iter() {
                let part_name = node_factory.literal_string(key);
                let required = node_factory.literal_boolean(*required);
                let literal_field = node_factory.literal_field(Some(part_name), Some(required));
                parts_list = Some(node_factory.argument_list(parts_list, Some(literal_field)));
            }
            let type_expression = ast::generate_type_expression(&mut node_factory, OBJECT, true);
            let literal_object = node_factory.literal_object(parts_list);
            (type_expression, literal_object)
        };
        let skin_parts = ast::generate_private_static_variable(&cx, type_expression, _SKINPARTS, Some(literal_object));
        let mut node_factory = nf.borrow_mut();
        let return_type_expression = ast::generate_getter_selector(&mut node_factory, OBJECT, true);
        let return_type = node_factory.type_expression(Some(return_type_expression), true, false, -1);
        let function_signature = node_factory.function_signature(None, Some(return_type));
        let identifier = ast::generate_identifier(&mut node_factory, _SKINPARTS, false);
        let selector = node_factory.get_expression(Some(identifier));
        let member_expression = node_factory.member_expression(None, Some(selector));
        let list = node_factory.list(None, Some(member_expression));
        let return_statement = node_factory.return_statement(Some(list));
        let r_statement_list = node_factory.statement_list(None, Some(return_statement));
        let property_getter = node_factory.function_common(&cx, None, Some(function_signature), Some(r_statement_list));
        let q_function_name = ast::generate_public_qualified_identifier(&mut node_factory, SKINPARTS);
        let function_name = node_factory.function_name(GET_TOKEN, Some(q_function_name));
        let protected = ast::generate_protected_attribute(&mut node_factory);
        let function_definition = node_factory.function_definition(&cx, Some(protected), Some(function_name), Some(property_getter));
        let override_ = ast::generate_getter_selector(&mut node_factory, OVERRIDE, false);
        let attrs = function_definition.borrow().function_definition().unwrap().def.attrs.clone();
        let attrs = node_factory.attribute_list(Some(override_), attrs);
        function_definition.borrow_mut().function_definition_mut().unwrap().def.attrs = Some(attrs);
        let result = node_factory.statement_list(statements, Some(skin_parts));
        node_factory.statement_list(Some(result), Some(function_definition))
    }
}

impl Adapter for SkinPartEvaluator<'_> {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let is_class = matches!(node.borrow().kind, Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_));
        if is_class {
            self.class_definition(cx, node);
        }
        is_class
    }
}
