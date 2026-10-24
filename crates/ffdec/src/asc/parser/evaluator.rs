//! Port of `macromedia/asc/parser/Evaluator.java`, plus the `evaluate()` overrides of every node class.

#![allow(non_upper_case_globals)]

use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::semantics::value::Value;
use crate::asc::util::context::ContextRef;

pub const global_this: i32 = 0;
pub const instance_this: i32 = 1;
pub const error_this: i32 = 2;
pub const cinit_this: i32 = 3;
pub const package_this: i32 = 4;
pub const init_this: i32 = 5;

pub const super_statement: i32 = 0;
pub const super_expression: i32 = 1;
pub const super_error: i32 = 2;
pub const super_error2: i32 = 3;
pub const super_error_es4: i32 = 4;

/// One method per Java `evaluate` overload; the defaults return what `Node.evaluate` does.
#[rustfmt::skip]
pub trait Evaluator {
    fn check_feature(&mut self, _cx: &ContextRef, _node: &NodeRef) -> bool { true }

    fn evaluate_node(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_increment(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_delete_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_identifier(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_invoke(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_this_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_qualified_identifier(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_qualified_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_boolean(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_number(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_string(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_null(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_reg_exp(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_xml(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_function_common(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_paren_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_paren_list_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_object(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_field(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_array(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_literal_vector(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_super_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_super_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_member_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_call_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_get_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_set_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_apply_type_expr(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_unary_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_binary_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_conditional_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_argument_list(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_list(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_statement_list(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_empty_element(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_empty_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_expression_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_labeled_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_if_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_switch_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_case_label(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_do_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_while_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_for_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_with_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_continue_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_break_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_return_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_throw_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_try_statement(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_catch_clause(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_finally_clause(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_use_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_include_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_import(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_meta_data(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_doc_comment(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_import_directive(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_attribute_list(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_variable_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_variable_binding(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_untyped_variable_binding(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_typed_identifier(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_type_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_function_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_binary_function_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_function_name(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_function_signature(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_parameter(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_parameter_list(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_rest_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_rest_parameter(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_interface_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_class_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_binary_class_def(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_binary_interface_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_class_name(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_inheritance(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_namespace_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_config_namespace_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_package_definition(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_package_identifiers(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_package_name(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_program(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_binary_program(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_error(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_to_object(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_load_register(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_store_register(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_register(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_has_next(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_box(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_coerce(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_pragma(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_use_precision(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_use_numeric(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_use_rounding(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_pragma_expression(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
    fn evaluate_default_xml_namespace(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> { None }
}

type EvalFn<E> = fn(&mut E, &ContextRef, &NodeRef) -> Option<Value>;

/// `node.evaluate(cx, evaluator)`: each class's override, resolved to the overload Java's static typing picks.
pub fn evaluate<E: Evaluator + ?Sized>(node: &NodeRef, cx: &ContextRef, evaluator: &mut E) -> Option<Value> {
    let f: EvalFn<E> = match &node.borrow().kind {
        Kind::ApplyTypeExpr(_) => E::evaluate_apply_type_expr,
        Kind::ArgumentList(_) => E::evaluate_argument_list,
        Kind::AttributeList(_) => E::evaluate_attribute_list,
        Kind::BinaryClassDef(_) => E::evaluate_binary_class_def,
        Kind::BinaryExpression(_) => E::evaluate_binary_expression,
        Kind::BinaryFunctionDefinition(_) => E::evaluate_binary_function_definition,
        Kind::BinaryInterfaceDefinition(_) => E::evaluate_binary_interface_definition,
        Kind::BinaryProgram(_) => E::evaluate_binary_program,
        Kind::Block(_) => E::evaluate_node,
        Kind::Box(_) => E::evaluate_box,
        Kind::BreakStatement(_) => E::evaluate_break_statement,
        Kind::CallExpression(_) => E::evaluate_call_expression,
        Kind::CaseLabel(_) => E::evaluate_case_label,
        Kind::CatchClause(_) => E::evaluate_catch_clause,
        Kind::ClassDefinition(_) => E::evaluate_class_definition,
        Kind::ClassName(_) => E::evaluate_class_name,
        Kind::Coerce(_) => E::evaluate_coerce,
        Kind::Comment(_) => return None,
        Kind::ConditionalExpression(_) => E::evaluate_conditional_expression,
        Kind::ConfigNamespaceDefinition(_) => E::evaluate_config_namespace_definition,
        Kind::ContinueStatement(_) => E::evaluate_continue_statement,
        Kind::DefaultXMLNamespace(_) => E::evaluate_default_xml_namespace,
        Kind::DeleteExpression(_) => E::evaluate_delete_expression,
        Kind::DoStatement(_) => E::evaluate_do_statement,
        Kind::DocComment(_) => E::evaluate_doc_comment,
        Kind::EmptyElement(_) => E::evaluate_empty_element,
        Kind::EmptyStatement(_) => return None,
        Kind::Error(_) => E::evaluate_error,
        Kind::ExpressionStatement(_) => E::evaluate_expression_statement,
        Kind::FinallyClause(_) => E::evaluate_finally_clause,
        Kind::ForStatement(_) => E::evaluate_for_statement,
        Kind::FunctionCommon(_) => E::evaluate_function_common,
        Kind::FunctionDefinition(_) => E::evaluate_function_definition,
        Kind::FunctionName(_) => E::evaluate_function_name,
        Kind::FunctionSignature(_) => E::evaluate_function_signature,
        Kind::GetExpression(_) => E::evaluate_get_expression,
        Kind::HasNext(_) => E::evaluate_has_next,
        Kind::IdentifierError(_) => E::evaluate_identifier,
        Kind::Identifier(_) => E::evaluate_identifier,
        Kind::IfStatement(_) => E::evaluate_if_statement,
        Kind::ImportDirective(_) => E::evaluate_import_directive,
        Kind::Import(_) => E::evaluate_import,
        Kind::IncludeDirective(_) => E::evaluate_include_directive,
        Kind::Increment(_) => E::evaluate_increment,
        Kind::Inheritance(_) => E::evaluate_inheritance,
        Kind::InterfaceDefinition(_) => E::evaluate_interface_definition,
        Kind::Invoke(_) => E::evaluate_invoke,
        Kind::LabeledStatement(_) => E::evaluate_labeled_statement,
        Kind::ListError(_) => E::evaluate_list,
        Kind::List(_) => E::evaluate_list,
        Kind::LiteralArray(_) => E::evaluate_literal_array,
        Kind::LiteralBoolean(_) => E::evaluate_literal_boolean,
        Kind::LiteralField(_) => E::evaluate_literal_field,
        Kind::LiteralNull(_) => E::evaluate_literal_null,
        Kind::LiteralNumber(_) => E::evaluate_literal_number,
        Kind::LiteralObject(_) => E::evaluate_literal_object,
        Kind::LiteralRegExp(_) => E::evaluate_literal_reg_exp,
        Kind::LiteralString(_) => E::evaluate_literal_string,
        Kind::LiteralVector(_) => E::evaluate_literal_vector,
        Kind::LiteralXML(_) => E::evaluate_literal_xml,
        Kind::LoadRegister(_) => E::evaluate_load_register,
        Kind::MemberExpression(_) => E::evaluate_member_expression,
        Kind::MetaData(_) => E::evaluate_meta_data,
        Kind::NamespaceDefinition(_) => E::evaluate_namespace_definition,
        Kind::PackageDefinition(_) => E::evaluate_package_definition,
        Kind::PackageIdentifiers(_) => E::evaluate_package_identifiers,
        Kind::PackageName(_) => E::evaluate_package_name,
        Kind::ParameterList(_) => E::evaluate_parameter_list,
        Kind::Parameter(_) => E::evaluate_parameter,
        Kind::ParenExpression(_) => E::evaluate_paren_expression,
        Kind::ParenListExpression(_) => E::evaluate_paren_list_expression,
        Kind::PragmaExpression(_) => E::evaluate_pragma_expression,
        Kind::Pragma(_) => E::evaluate_pragma,
        Kind::Program(_) => E::evaluate_program,
        Kind::QualifiedExpression(_) => E::evaluate_qualified_expression,
        Kind::QualifiedIdentifier(_) => E::evaluate_qualified_identifier,
        Kind::Register(_) => E::evaluate_register,
        Kind::RestExpression(_) => E::evaluate_rest_expression,
        Kind::RestParameter(_) => E::evaluate_rest_parameter,
        Kind::ReturnStatement(_) => E::evaluate_return_statement,
        Kind::SetExpression(_) => E::evaluate_set_expression,
        Kind::StatementList(_) => E::evaluate_statement_list,
        Kind::StoreRegister(_) => E::evaluate_store_register,
        Kind::SuperExpression(_) => E::evaluate_super_expression,
        Kind::SuperStatement(_) => E::evaluate_super_statement,
        Kind::SwitchStatement(_) => E::evaluate_switch_statement,
        Kind::ThisExpression(_) => E::evaluate_this_expression,
        Kind::ThrowStatement(_) => E::evaluate_throw_statement,
        Kind::ToObject(_) => E::evaluate_to_object,
        Kind::TryStatement(_) => E::evaluate_try_statement,
        Kind::TypeExpression(_) => E::evaluate_type_expression,
        Kind::TypeIdentifier(_) => E::evaluate_identifier,
        Kind::TypedIdentifier(_) => E::evaluate_typed_identifier,
        Kind::UnaryExpression(_) => E::evaluate_unary_expression,
        Kind::UntypedVariableBinding(_) => E::evaluate_untyped_variable_binding,
        Kind::UseDirective(_) => E::evaluate_use_directive,
        Kind::UseNumeric(_) => E::evaluate_use_numeric,
        Kind::UsePragma(_) => E::evaluate_node,
        Kind::UsePrecision(_) => E::evaluate_use_precision,
        Kind::UseRounding(_) => E::evaluate_use_rounding,
        Kind::VariableBinding(_) => E::evaluate_variable_binding,
        Kind::VariableDefinition(_) => E::evaluate_variable_definition,
        Kind::WhileStatement(_) => E::evaluate_while_statement,
        Kind::WithStatement(_) => E::evaluate_with_statement,
    };
    if evaluator.check_feature(cx, node) {
        f(evaluator, cx, node)
    } else {
        None
    }
}
