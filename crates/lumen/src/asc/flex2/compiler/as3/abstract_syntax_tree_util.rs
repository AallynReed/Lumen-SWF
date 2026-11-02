//! Port of `flex2.compiler.as3.AbstractSyntaxTreeUtil` and `CodeFragmentLogAdapter`: helpers for direct AST
//! generation. Java overloads get suffixes naming their extra parameters.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::as3_compiler::As3CompilerHandler;
use crate::asc::flex2::compiler::source::SourceRef;
use crate::asc::flex2::compiler::toolkit::{self, LogAdapter, LogCall};
use crate::asc::flex2::compiler::util::QName;
use crate::asc::parser::factory::NodeFactory;
use crate::asc::parser::input::InputBuffer;
use crate::asc::parser::node::{ExpressionStatementNode, ListNode, NodeRef};
use crate::asc::parser::parser::Parser;
use crate::asc::parser::tokens::{CLASS_TOKEN, EMPTY_TOKEN, EOS_TOKEN, PACKAGE_TOKEN, VAR_TOKEN};
use crate::asc::semantics::namespace_value::new_public_namespace_value;
use crate::asc::semantics::reference_value::ReferenceValue;
use crate::asc::semantics::emitter::EmitterRef;
use crate::asc::util::context::{ConfigVar, Context, ContextRef, ContextStatics};
use crate::asc::util::namespaces::Namespaces;

const DOT: char = '.';
const DOUBLE_COLON: &str = "::";
const LESS_THAN: char = '<';
const GREATER_THAN: char = '>';

const __AS3__: &str = "__AS3__";
const CONFIG: &str = "CONFIG";
const MX_INTERNAL: &str = "mx_internal";
const OVERRIDE: &str = "override";
const PRIVATE: &str = "private";
const PROTECTED: &str = "protected";
const PRIVATE_DOC_COMMENT: &str = "<description><![CDATA[\n ]]></description>\n<private><![CDATA[ ]]></private>";
const INHERIT_DOC_COMMENT: &str = "<description><![CDATA[\n ]]></description>\n<inheritDoc><![CDATA[ ]]></inheritDoc>";
const PUBLIC: &str = "public";
const STATIC: &str = "static";
const VEC: &str = "vec";
const VECTOR: &str = "Vector";
const CORE_PACKAGE: &str = "mx.core";

/// A context's node factory, borrowed afresh for each call so generation can interleave with parsing.
#[derive(Clone)]
pub struct Nf(pub Rc<RefCell<NodeFactory>>);

impl Nf {
    pub fn of(cx: &ContextRef) -> Nf {
        Nf(Context::get_node_factory(cx))
    }

    pub fn f(&self) -> std::cell::RefMut<'_, NodeFactory> {
        self.0.borrow_mut()
    }
}

/// `callExpression(expr, args)` with `setRValue(false)`, and `is_new` when `is_new`.
pub fn call(nf: &Nf, expr: NodeRef, args: Option<NodeRef>, is_new: bool) -> NodeRef {
    let c = nf.f().call_expression(Some(expr), args);
    {
        let mut b = c.borrow_mut();
        if is_new {
            b.as_call_expression_mut().unwrap().is_new = true;
        }
        b.selector_mut().unwrap().set_rvalue(false);
    }
    c
}

/// `memberExpression(base, selector)` wrapped in `list(null, ...)` and `expressionStatement(...)`.
pub fn expr_stmt(nf: &Nf, base: Option<NodeRef>, selector: NodeRef) -> NodeRef {
    let m = nf.f().member_expression(base, Some(selector));
    let list = nf.f().list(None, Some(m));
    nf.f().expression_statement(Some(list))
}

/// Sets a selector's mode, as `setMode(mode)`.
pub fn set_mode(node: &NodeRef, mode: i32) {
    node.borrow_mut().selector_mut().unwrap().set_mode(mode);
}

/// Sets a selector's rvalue flag off, as `setRValue(false)`.
pub fn no_rvalue(node: &NodeRef) {
    node.borrow_mut().selector_mut().unwrap().set_rvalue(false);
}

/// `functionSignature.void_anno = true`.
pub fn void_anno(signature: &NodeRef) {
    signature.borrow_mut().as_function_signature_mut().unwrap().void_anno = true;
}

/// `functionCommon.setUserDefinedBody(true)`.
pub fn user_defined_body(function_common: &NodeRef) {
    function_common.borrow_mut().as_function_common_mut().unwrap().set_user_defined_body(true);
}

/// `generateApplyTypeExpr(nodeFactory, type)`.
pub fn generate_apply_type_expr(node_factory: &mut NodeFactory, type_: &str) -> NodeRef {
    let identifier = generate_identifier(node_factory, VECTOR, false);
    let type_expression = generate_type_expression_any(node_factory, type_, true, true);
    let type_args = node_factory.list(None, type_expression);
    node_factory.apply_type_expr(Some(identifier), Some(type_args), -1)
}

/// `generateApplyTypeExpr(nodeFactory, type, lessThanIndex)`.
pub fn generate_apply_type_expr_at(node_factory: &mut NodeFactory, type_: &str, less_than_index: usize) -> NodeRef {
    let vector_type = &type_[..less_than_index - 1];
    let identifier = match vector_type.rfind(DOT).filter(|&i| i > 0) {
        Some(dot_index) => generate_qualified_identifier(node_factory, &vector_type[..dot_index], &vector_type[dot_index + 1..], true),
        None => generate_identifier(node_factory, vector_type, true),
    };
    let greater_than_index = type_.rfind(GREATER_THAN).expect("StringIndexOutOfBoundsException");
    let element_type = &type_[less_than_index + 1..greater_than_index];
    let type_expression = generate_type_expression_full(node_factory, element_type, true, true, -1);
    let type_args = node_factory.list(None, type_expression);
    node_factory.apply_type_expr(Some(identifier), Some(type_args), -1)
}

/// `generateAssignment(nodeFactory, String lvalue, String rvalue)`.
pub fn generate_assignment(node_factory: &mut NodeFactory, lvalue: &str, rvalue: &str) -> NodeRef {
    let lvalue_identifier = node_factory.identifier(lvalue);
    let rvalue_identifier = node_factory.identifier(rvalue);
    let get_expression = node_factory.get_expression(Some(rvalue_identifier));
    let rvalue_member_expression = node_factory.member_expression(None, Some(get_expression));
    let argument_list = node_factory.argument_list(None, Some(rvalue_member_expression));
    let set_expression = node_factory.set_expression_pos(Some(lvalue_identifier), Some(argument_list), false, 1);
    let member_expression = node_factory.member_expression(None, Some(set_expression));
    let list = node_factory.list(None, Some(member_expression));
    node_factory.expression_statement(Some(list))
}

/// `generateAssignment(nodeFactory, Node base, String lvalue, Node rvalue)`.
pub fn generate_assignment_base(node_factory: &mut NodeFactory, base: Option<NodeRef>, lvalue: &str, rvalue: Option<NodeRef>) -> NodeRef {
    let lvalue_identifier = node_factory.identifier(lvalue);
    let argument_list = node_factory.argument_list(None, rvalue);
    let set_expression = node_factory.set_expression_pos(Some(lvalue_identifier), Some(argument_list), false, 1);
    let member_expression = node_factory.member_expression(base, Some(set_expression));
    let list = node_factory.list(None, Some(member_expression));
    node_factory.expression_statement(Some(list))
}

/// `generateClassDefinition(context, className, baseClassName, interfaceNames, statementList)`.
pub fn generate_class_definition(context: &ContextRef, class_name: &str, base_class_name: &str, interface_names: &[String], statement_list: Option<NodeRef>) -> NodeRef {
    let nf = Context::get_node_factory(context);
    let mut node_factory = nf.borrow_mut();
    node_factory.start_class_defs();
    let mut interfaces: Option<NodeRef> = None;
    for interface_name in interface_names {
        let selector = match interface_name.rfind(DOT).filter(|&i| i > 0) {
            Some(index) => generate_getter_selector_qualified(&mut node_factory, &interface_name[..index], &interface_name[index + 1..], true),
            None => generate_getter_selector(&mut node_factory, interface_name, true),
        };
        interfaces = Some(node_factory.list(interfaces, Some(selector)));
    }
    let base = match base_class_name.rfind(DOT).filter(|&i| i > 0) {
        Some(index) => generate_getter_selector_qualified(&mut node_factory, &base_class_name[..index], &base_class_name[index + 1..], true),
        None => generate_getter_selector(&mut node_factory, base_class_name, true),
    };
    let inheritance = node_factory.inheritance(Some(base), interfaces);
    let attrs = generate_public_attribute(&mut node_factory);
    let name = generate_public_qualified_identifier(&mut node_factory, class_name);
    node_factory.class_definition(context, Some(attrs), Some(name), Some(inheritance), statement_list)
}

/// `generateConstructor(context, className, parameterList, generateSuperCall, statementList, position)`.
pub fn generate_constructor(context: &ContextRef, class_name: &str, parameter_list: Option<NodeRef>, generate_super_call: bool, mut statement_list: Option<NodeRef>, position: i32) -> NodeRef {
    let nf = Context::get_node_factory(context);
    let mut node_factory = nf.borrow_mut();
    let attribute_list = generate_public_attribute(&mut node_factory);
    let identifier = node_factory.identifier(class_name);
    let function_name = node_factory.function_name(EMPTY_TOKEN, Some(identifier.clone()));
    let function_signature = node_factory.function_signature(parameter_list, None);
    function_signature.borrow_mut().as_function_signature_mut().unwrap().no_anno = true;
    if generate_super_call {
        let super_expression = node_factory.super_expression(None, -1);
        let call_expression = node_factory.call_expression(Some(super_expression), None);
        let super_statement = node_factory.super_statement(Some(call_expression));
        match &statement_list {
            Some(s) => s.borrow_mut().as_statement_list_mut().unwrap().items.insert(0, Some(super_statement)),
            None => statement_list = Some(node_factory.statement_list(None, Some(super_statement))),
        }
    }
    let function_common = node_factory.function_common_pos(context, Some(identifier), Some(function_signature), statement_list, position);
    function_common.borrow_mut().as_function_common_mut().unwrap().set_user_defined_body(true);
    node_factory.function_definition(context, Some(attribute_list), Some(function_name), Some(function_common))
}

/// `generateContext(contextStatics, source, emitter, defines)`.
pub fn generate_context(context_statics: &Rc<RefCell<ContextStatics>>, source: &SourceRef, emitter: &EmitterRef, defines: &[ConfigVar]) -> ContextRef {
    let result = Rc::new(RefCell::new(Context::new(context_statics.clone())));
    let mut backing = source.clone();
    loop {
        let d = backing.borrow().get_delegate();
        match d {
            Some(d) => backing = d,
            None => break,
        }
    }
    let (name, parent) = {
        let s = backing.borrow();
        (s.get_name().to_string(), s.get_parent())
    };
    result.borrow_mut().set_origin(&name);
    {
        let c = result.borrow();
        c.set_script_name(Some(name.clone()));
        c.set_path(parent);
        c.set_emitter(Some(emitter.clone()));
    }
    result.borrow_mut().set_handler(Rc::new(RefCell::new(As3CompilerHandler { source: Some(Rc::downgrade(source)) })));
    result.borrow_mut().input = Some(Rc::new(RefCell::new(InputBuffer::new_code_fragments(&name))));
    result.borrow_mut().config_vars.extend(defines.iter().cloned());
    result
}

/// `generateDocComment(nodeFactory, comment)`.
pub fn generate_doc_comment(node_factory: &mut NodeFactory, comment: &str) -> NodeRef {
    let literal_string = node_factory.literal_string_intern(comment, false);
    let list = node_factory.list(None, Some(literal_string));
    let literal_xml = node_factory.literal_xml(Some(list), false, -1);
    let get_expression = node_factory.get_expression(Some(literal_xml));
    let member_expression = node_factory.member_expression(None, Some(get_expression));
    let argument_list = node_factory.argument_list(None, Some(member_expression));
    let literal_array = node_factory.literal_array(Some(argument_list));
    node_factory.doc_comment(Some(literal_array), -1)
}

/// `generateGetterSelector(nodeFactory, name, intern)`.
pub fn generate_getter_selector(node_factory: &mut NodeFactory, name: &str, intern: bool) -> NodeRef {
    generate_getter_selector_pos(node_factory, name, intern, -1)
}

/// `generateGetterSelector(nodeFactory, name, intern, position)`.
pub fn generate_getter_selector_pos(node_factory: &mut NodeFactory, name: &str, intern: bool, position: i32) -> NodeRef {
    let identifier = node_factory.identifier_intern_pos(name, intern, position);
    let get_expression = node_factory.get_expression(Some(identifier));
    node_factory.member_expression(None, Some(get_expression))
}

/// `generateGetterSelector(nodeFactory, String qualifier, name, intern)`.
pub fn generate_getter_selector_qualified(node_factory: &mut NodeFactory, qualifier: &str, name: &str, intern: bool) -> NodeRef {
    let qualified_identifier = generate_qualified_identifier(node_factory, qualifier, name, intern);
    let get_expression = node_factory.get_expression(Some(qualified_identifier));
    node_factory.member_expression(None, Some(get_expression))
}

/// `generateGetterSelector(nodeFactory, MemberExpressionNode qualifier, name, intern)`.
pub fn generate_getter_selector_member(node_factory: &mut NodeFactory, qualifier: NodeRef, name: &str, intern: bool) -> NodeRef {
    let qualified_identifier = generate_qualified_identifier_member(node_factory, qualifier, name, intern);
    let get_expression = node_factory.get_expression(Some(qualified_identifier));
    node_factory.member_expression(None, Some(get_expression))
}

/// `generateIdentifier(nodeFactory, String name, intern)`.
pub fn generate_identifier(node_factory: &mut NodeFactory, name: &str, intern: bool) -> NodeRef {
    if let Some(index) = name.rfind(DOT) {
        generate_qualified_identifier(node_factory, &name[..index], &name[index + 1..], true)
    } else if let Some(index) = name.find(DOUBLE_COLON) {
        let member_expression = generate_getter_selector(node_factory, &name[..index], true);
        generate_qualified_identifier_member(node_factory, member_expression, &name[index + 2..], true)
    } else {
        node_factory.identifier_intern(name, intern)
    }
}

/// `generateIdentifier(nodeFactory, QName qName, intern)`.
pub fn generate_identifier_qname(node_factory: &mut NodeFactory, q_name: &QName, intern: bool) -> NodeRef {
    if !q_name.get_namespace().is_empty() {
        let member_expression = generate_getter_selector(node_factory, q_name.get_namespace(), intern);
        generate_qualified_identifier_member(node_factory, member_expression, q_name.get_local_part(), intern)
    } else {
        node_factory.identifier_intern(q_name.get_local_part(), intern)
    }
}

fn generate_implicit_namespaces(context: &ContextRef, statement_list: NodeRef) -> NodeRef {
    let mut result = statement_list;
    let nf = Context::get_node_factory(context);
    let (es4_vectors, use_namespaces) = {
        let c = context.borrow();
        let s = c.statics.borrow();
        (s.es4_vectors, s.use_namespaces.clone())
    };
    let mut node_factory = nf.borrow_mut();
    if es4_vectors {
        let as3_identifier = node_factory.identifier_intern(__AS3__, false);
        let package_identifiers = node_factory.package_identifiers(None, Some(as3_identifier), true);
        let vec_identifier = node_factory.identifier_intern(VEC, false);
        let package_identifiers = node_factory.package_identifiers(Some(package_identifiers), Some(vec_identifier), true);
        let vector_identifier = node_factory.identifier_intern(VECTOR, false);
        let package_identifiers = node_factory.package_identifiers(Some(package_identifiers), Some(vector_identifier), true);
        let package_name = node_factory.package_name(Some(package_identifiers));
        let import_directive = node_factory.import_directive(None, Some(package_name), None, context);
        result = node_factory.statement_list(Some(result), Some(import_directive));
    }
    for use_name in &use_namespaces {
        let identifier = node_factory.identifier(use_name);
        let get_expression = node_factory.get_expression(Some(identifier));
        let member_expression = node_factory.member_expression(None, Some(get_expression));
        let use_directive = node_factory.use_directive(None, Some(member_expression));
        result = node_factory.statement_list(Some(result), Some(use_directive));
    }
    drop(node_factory);
    let use_directive = Parser::generate_as3_use_directive(context);
    let result = nf.borrow_mut().statement_list(Some(result), Some(use_directive));
    result
}

/// `generateImport(context, String name)`.
pub fn generate_import(context: &ContextRef, name: &str) -> NodeRef {
    generate_import_pos(context, name, -1)
}

/// `generateImport(context, String[] splitName)`.
pub fn generate_import_split(context: &ContextRef, split_name: &[String]) -> NodeRef {
    generate_import_split_pos(context, split_name, -1)
}

/// `generateImport(context, String name, position)`.
pub fn generate_import_pos(context: &ContextRef, name: &str, position: i32) -> NodeRef {
    let nf = Context::get_node_factory(context);
    let mut node_factory = nf.borrow_mut();
    let package_name = generate_package_name(&mut node_factory, name, true, position);
    node_factory.import_directive(None, Some(package_name), None, context)
}

/// `generateImport(context, String[] splitName, position)`.
pub fn generate_import_split_pos(context: &ContextRef, split_name: &[String], position: i32) -> NodeRef {
    let nf = Context::get_node_factory(context);
    let mut node_factory = nf.borrow_mut();
    let package_name = generate_package_name_split(&mut node_factory, split_name, true, position);
    node_factory.import_directive(None, Some(package_name), None, context)
}

/// `generateMemberExpression(nodeFactory, expression, position)`.
pub fn generate_member_expression_pos(node_factory: &mut NodeFactory, expression: &str, position: i32) -> NodeRef {
    if let Some(less_than_index) = expression.find(LESS_THAN) {
        let vector_string = &expression[..less_than_index - 1];
        let base = generate_getter_selector(node_factory, vector_string, true);
        let element_string = &expression[less_than_index + 1..expression.len() - 1];
        let selector = generate_apply_type_expr(node_factory, element_string);
        selector.borrow_mut().selector_mut().unwrap().set_rvalue(false);
        node_factory.member_expression_pos(Some(base), Some(selector), position)
    } else if let Some(last_dot_index) = expression.rfind(DOT) {
        let base_string = &expression[..last_dot_index];
        let base = generate_getter_selector_pos(node_factory, base_string, true, position);
        let selector_string = &expression[last_dot_index + 1..];
        let identifier = node_factory.identifier_pos(selector_string, position);
        let selector = node_factory.get_expression_pos(Some(identifier), position);
        selector.borrow_mut().selector_mut().unwrap().set_rvalue(false);
        let result = node_factory.member_expression_pos(Some(base), Some(selector), position);
        result.borrow_mut().set_position_terminal(position);
        result
    } else {
        generate_getter_selector_pos(node_factory, expression, true, position)
    }
}

/// `generateMemberExpression(nodeFactory, expression)`.
pub fn generate_member_expression(node_factory: &mut NodeFactory, expression: &str) -> NodeRef {
    generate_member_expression_pos(node_factory, expression, -1)
}

/// `generateMetaData(nodeFactory, name)`.
pub fn generate_meta_data(node_factory: &mut NodeFactory, name: &str) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let get_expression = node_factory.get_expression(Some(identifier));
    let member_expression = node_factory.member_expression(None, Some(get_expression));
    let argument_list = node_factory.argument_list(None, Some(member_expression));
    let literal_array = node_factory.literal_array(Some(argument_list));
    node_factory.meta_data(Some(literal_array), -1)
}

/// `generateMetaData(nodeFactory, name, value)`.
pub fn generate_meta_data_value(node_factory: &mut NodeFactory, name: &str, value: &str) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let literal_string = node_factory.literal_string_intern(value, false);
    let call_expression_argument_list = node_factory.argument_list(None, Some(literal_string));
    let call_expression = node_factory.call_expression(Some(identifier), Some(call_expression_argument_list));
    call_expression.borrow_mut().selector_mut().unwrap().set_rvalue(false);
    let member_expression = node_factory.member_expression(None, Some(call_expression));
    let argument_list = node_factory.argument_list(None, Some(member_expression));
    let literal_array = node_factory.literal_array(Some(argument_list));
    node_factory.meta_data(Some(literal_array), -1)
}

/// `generateMetaData(nodeFactory, name, key, value)`.
pub fn generate_meta_data_key_value(node_factory: &mut NodeFactory, name: &str, key: &str, value: &str) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let attribute_identifier = node_factory.identifier_intern(key, false);
    let literal_string = node_factory.literal_string(value);
    let attribute_argument_list = node_factory.argument_list(None, Some(literal_string));
    let set_expression = node_factory.set_expression(Some(attribute_identifier), Some(attribute_argument_list), false);
    let key_value_member_expression = node_factory.member_expression(None, Some(set_expression));
    let call_expression_argument_list = node_factory.argument_list(None, Some(key_value_member_expression));
    let call_expression = node_factory.call_expression(Some(identifier), Some(call_expression_argument_list));
    call_expression.borrow_mut().selector_mut().unwrap().set_rvalue(false);
    let member_expression = node_factory.member_expression(None, Some(call_expression));
    let argument_list = node_factory.argument_list(None, Some(member_expression));
    let literal_array = node_factory.literal_array(Some(argument_list));
    node_factory.meta_data(Some(literal_array), -1)
}

/// `generateMetaData(nodeFactory, name, Map attributes)`; `attributes` in the map's iteration order.
pub fn generate_meta_data_attributes(node_factory: &mut NodeFactory, name: &str, attributes: &[(String, String)]) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let mut call_expression_argument_list: Option<NodeRef> = None;
    for (key, value) in attributes {
        let attribute_identifier = node_factory.identifier(key);
        let literal_string = node_factory.literal_string(value);
        let attribute_argument_list = node_factory.argument_list(None, Some(literal_string));
        let set_expression = node_factory.set_expression(Some(attribute_identifier), Some(attribute_argument_list), false);
        let member_expression = node_factory.member_expression(None, Some(set_expression));
        call_expression_argument_list = Some(node_factory.argument_list(call_expression_argument_list, Some(member_expression)));
    }
    let call_expression = node_factory.call_expression(Some(identifier), call_expression_argument_list);
    call_expression.borrow_mut().selector_mut().unwrap().set_rvalue(false);
    let member_expression = node_factory.member_expression(None, Some(call_expression));
    let argument_list = node_factory.argument_list(None, Some(member_expression));
    let literal_array = node_factory.literal_array(Some(argument_list));
    node_factory.meta_data(Some(literal_array), -1)
}

/// `generateMxInternalQualifiedIdentifier(nodeFactory, name, intern)`.
pub fn generate_mx_internal_qualified_identifier(node_factory: &mut NodeFactory, name: &str, _intern: bool) -> NodeRef {
    let mx_internal_getter_selector = generate_resolved_getter_selector(node_factory, CORE_PACKAGE, MX_INTERNAL);
    node_factory.qualified_identifier(Some(mx_internal_getter_selector), Some(name))
}

pub fn generate_override_public_attribute(node_factory: &mut NodeFactory) -> NodeRef {
    let identifier = node_factory.identifier_intern(PUBLIC, false);
    let attribute_list = node_factory.attribute_list(Some(identifier), None);
    let member_expression = generate_getter_selector(node_factory, OVERRIDE, false);
    let list = node_factory.list(None, Some(member_expression));
    node_factory.attribute_list(Some(list), Some(attribute_list))
}

/// `generatePackageName(nodeFactory, String name, isDefinition, position)`.
pub fn generate_package_name(node_factory: &mut NodeFactory, name: &str, is_definition: bool, position: i32) -> NodeRef {
    let mut package_identifiers: Option<NodeRef> = None;
    let tokens: Vec<&str> = java_scanner_split(name);
    if !tokens.is_empty() {
        for token in tokens {
            let identifier = node_factory.identifier(token);
            package_identifiers = Some(node_factory.package_identifiers(package_identifiers, Some(identifier), is_definition));
        }
    } else {
        let identifier = node_factory.identifier(name);
        package_identifiers = Some(node_factory.package_identifiers(package_identifiers, Some(identifier), is_definition));
    }
    node_factory.package_name_pos(package_identifiers, position)
}

/// The tokens `new Scanner(name).useDelimiter("\\.")` yields: empty tokens between delimiters are kept, a leading
/// empty one too, but not a trailing one.
fn java_scanner_split(name: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = name.split(DOT).collect();
    if tokens.last().is_some_and(|t| t.is_empty()) {
        tokens.pop();
    }
    tokens
}

/// `generatePackageName(nodeFactory, String[] splitName, isDefinition, position)`.
pub fn generate_package_name_split(node_factory: &mut NodeFactory, split_name: &[String], is_definition: bool, position: i32) -> NodeRef {
    let mut package_identifiers: Option<NodeRef> = None;
    for name in split_name {
        let identifier = node_factory.identifier_intern(name, false);
        package_identifiers = Some(node_factory.package_identifiers(package_identifiers, Some(identifier), is_definition));
    }
    node_factory.package_name_pos(package_identifiers, position)
}

/// `generateParameter(nodeFactory, name, type, internType)`.
pub fn generate_parameter(node_factory: &mut NodeFactory, name: &str, type_: &str, intern_type: bool) -> NodeRef {
    generate_parameter_pos(node_factory, name, type_, intern_type, -1)
}

/// `generateParameter(nodeFactory, name, type, internType, position)`.
pub fn generate_parameter_pos(node_factory: &mut NodeFactory, name: &str, type_: &str, intern_type: bool, _position: i32) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let type_expression = generate_type_expression(node_factory, type_, intern_type);
    node_factory.parameter(VAR_TOKEN, Some(identifier), type_expression)
}

/// `generateParameter(nodeFactory, name, type, internType, Node init)`.
pub fn generate_parameter_init(node_factory: &mut NodeFactory, name: &str, type_: &str, intern_type: bool, init: Option<NodeRef>) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let type_expression = generate_type_expression(node_factory, type_, intern_type);
    node_factory.parameter_init(VAR_TOKEN, Some(identifier), type_expression, init)
}

/// `generateParameter(nodeFactory, name, typeNamespace, typeName, internType)`.
pub fn generate_parameter_ns(node_factory: &mut NodeFactory, name: &str, type_namespace: &str, type_name: &str, intern_type: bool) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let type_expression = generate_type_expression_ns(node_factory, type_namespace, type_name, intern_type);
    node_factory.parameter(VAR_TOKEN, Some(identifier), Some(type_expression))
}

pub fn generate_private_doc_comment(node_factory: &mut NodeFactory) -> NodeRef {
    generate_doc_comment(node_factory, PRIVATE_DOC_COMMENT)
}

pub fn generate_inherit_doc_comment(node_factory: &mut NodeFactory) -> NodeRef {
    generate_doc_comment(node_factory, INHERIT_DOC_COMMENT)
}

/// `generatePrivateVariable(nodeFactory, typeExpression, name)`.
pub fn generate_private_variable(node_factory: &mut NodeFactory, type_expression: Option<NodeRef>, name: &str) -> NodeRef {
    generate_private_variable_init(node_factory, type_expression, name, None)
}

/// `generatePrivateVariable(nodeFactory, typeExpression, name, initializer)`.
pub fn generate_private_variable_init(node_factory: &mut NodeFactory, type_expression: Option<NodeRef>, name: &str, initializer: Option<NodeRef>) -> NodeRef {
    let attribute_list = generate_private_attribute(node_factory);
    let qualified_identifier = generate_private_qualified_identifier(node_factory, name);
    let typed_identifier = node_factory.typed_identifier(Some(qualified_identifier), type_expression);
    let variable_binding = node_factory.variable_binding(Some(attribute_list.clone()), VAR_TOKEN, Some(typed_identifier), initializer);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(Some(attribute_list), VAR_TOKEN, Some(list))
}

pub fn generate_private_attribute(node_factory: &mut NodeFactory) -> NodeRef {
    let sel = generate_getter_selector(node_factory, PRIVATE, false);
    let list = node_factory.list(None, Some(sel));
    node_factory.attribute_list(Some(list), None)
}

pub fn generate_private_qualified_identifier(node_factory: &mut NodeFactory, name: &str) -> NodeRef {
    let attrs = generate_private_attribute(node_factory);
    node_factory.qualified_identifier(Some(attrs), Some(name))
}

pub fn generate_private_static_attribute(node_factory: &mut NodeFactory) -> NodeRef {
    let member_expression = generate_getter_selector(node_factory, STATIC, false);
    let attribute_list = node_factory.attribute_list(Some(member_expression), None);
    let sel = generate_getter_selector(node_factory, PRIVATE, false);
    let list = node_factory.list(None, Some(sel));
    node_factory.attribute_list(Some(list), Some(attribute_list))
}

/// `generateProgram(context, configVars, packageNameString)`.
pub fn generate_program(context: &ContextRef, config_vars: Option<NodeRef>, package_name_string: &str) -> NodeRef {
    generate_program_full(context, config_vars, package_name_string, None, -1)
}

/// `generateProgram(context, configVars, packageNameString, packageDocComment, lineNumber)`.
pub fn generate_program_full(context: &ContextRef, config_vars: Option<NodeRef>, package_name_string: &str, package_doc_comment: Option<NodeRef>, line_number: i32) -> NodeRef {
    let nf = Context::get_node_factory(context);
    let mut statement_list = {
        let mut node_factory = nf.borrow_mut();
        let config_identifier = node_factory.identifier_intern(CONFIG, false);
        let config_namespace_definition = node_factory.config_namespace_definition(None, Some(config_identifier), -1);
        node_factory.statement_list(None, Some(config_namespace_definition))
    };
    if !context.borrow().config_vars.is_empty() {
        let items = config_vars.as_ref().map(|c| c.borrow().as_statement_list().unwrap().items.clone()).unwrap_or_default();
        let mut s = statement_list.borrow_mut();
        let list = s.as_statement_list_mut().unwrap();
        for (i, item) in items.into_iter().enumerate() {
            list.items.insert(i, item);
        }
    }
    statement_list = generate_implicit_namespaces(context, statement_list);
    {
        let mut node_factory = nf.borrow_mut();
        if let Some(doc) = package_doc_comment {
            statement_list = node_factory.statement_list(Some(statement_list), Some(doc));
        }
        let package_name = generate_package_name(&mut node_factory, package_name_string, false, -1);
        let package_definition = node_factory.start_package(context, None, Some(package_name));
        statement_list = node_factory.statement_list(Some(statement_list), Some(package_definition));
    }
    statement_list = generate_implicit_namespaces(context, statement_list);
    let mut node_factory = nf.borrow_mut();
    let position = if line_number != -1 { line_number_to_position(&mut node_factory, line_number) } else { -1 };
    node_factory.program_pos(context, Some(statement_list), position)
}

pub fn generate_public_attribute(node_factory: &mut NodeFactory) -> NodeRef {
    let sel = generate_getter_selector(node_factory, PUBLIC, false);
    let list = node_factory.list(None, Some(sel));
    node_factory.attribute_list(Some(list), None)
}

pub fn generate_protected_attribute(node_factory: &mut NodeFactory) -> NodeRef {
    let sel = generate_getter_selector(node_factory, PROTECTED, false);
    let list = node_factory.list(None, Some(sel));
    node_factory.attribute_list(Some(list), None)
}

pub fn generate_public_qualified_identifier(node_factory: &mut NodeFactory, name: &str) -> NodeRef {
    let attrs = generate_public_attribute(node_factory);
    node_factory.qualified_identifier(Some(attrs), Some(name))
}

pub fn generate_protected_qualified_identifier(node_factory: &mut NodeFactory, name: &str) -> NodeRef {
    let attrs = generate_protected_attribute(node_factory);
    node_factory.qualified_identifier(Some(attrs), Some(name))
}

pub fn generate_public_static_attribute(node_factory: &mut NodeFactory) -> NodeRef {
    let member_expression = generate_getter_selector(node_factory, STATIC, false);
    let attribute_list = node_factory.attribute_list(Some(member_expression), None);
    let sel = generate_getter_selector(node_factory, PUBLIC, false);
    let list = node_factory.list(None, Some(sel));
    node_factory.attribute_list(Some(list), Some(attribute_list))
}

/// `generatePublicVariable(context, typeExpression, name)`.
pub fn generate_public_variable(context: &ContextRef, type_expression: Option<NodeRef>, name: &str) -> NodeRef {
    generate_public_variable_init(context, type_expression, name, None)
}

/// `generatePublicVariable(context, typeExpression, name, initializer)`.
pub fn generate_public_variable_init(context: &ContextRef, type_expression: Option<NodeRef>, name: &str, initializer: Option<NodeRef>) -> NodeRef {
    let nf = Context::get_node_factory(context);
    let mut node_factory = nf.borrow_mut();
    let attribute_list = generate_public_attribute(&mut node_factory);
    let qualified_identifier = generate_public_qualified_identifier(&mut node_factory, name);
    let typed_identifier = node_factory.typed_identifier(Some(qualified_identifier), type_expression);
    let variable_binding = node_factory.variable_binding(Some(attribute_list.clone()), VAR_TOKEN, Some(typed_identifier), initializer);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(Some(attribute_list), VAR_TOKEN, Some(list))
}

/// `generatePrivateStaticVariable(context, typeExpression, name, initializer)`.
pub fn generate_private_static_variable(context: &ContextRef, type_expression: Option<NodeRef>, name: &str, initializer: Option<NodeRef>) -> NodeRef {
    let nf = Context::get_node_factory(context);
    let mut node_factory = nf.borrow_mut();
    let attribute_list = generate_private_static_attribute(&mut node_factory);
    let qualified_identifier = generate_public_qualified_identifier(&mut node_factory, name);
    let typed_identifier = node_factory.typed_identifier(Some(qualified_identifier), type_expression);
    let variable_binding = node_factory.variable_binding(Some(attribute_list.clone()), VAR_TOKEN, Some(typed_identifier), initializer);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(Some(attribute_list), VAR_TOKEN, Some(list))
}

/// `generateQualifiedIdentifier(nodeFactory, String qualifier, name, intern)`.
pub fn generate_qualified_identifier(node_factory: &mut NodeFactory, qualifier: &str, name: &str, intern: bool) -> NodeRef {
    let literal_string = node_factory.literal_string_intern(qualifier, intern);
    node_factory.qualified_identifier(Some(literal_string), Some(name))
}

/// `generateQualifiedIdentifier(nodeFactory, MemberExpressionNode qualifier, name, intern)`.
pub fn generate_qualified_identifier_member(node_factory: &mut NodeFactory, qualifier: NodeRef, name: &str, _intern: bool) -> NodeRef {
    node_factory.qualified_identifier(Some(qualifier), Some(name))
}

/// `generateResolvedGetterSelector(nodeFactory, namespace, name)`.
pub fn generate_resolved_getter_selector(node_factory: &mut NodeFactory, namespace: &str, name: &str) -> NodeRef {
    let identifier = generate_resolved_identifier(node_factory, namespace, name);
    let get_expression = node_factory.get_expression(Some(identifier));
    node_factory.member_expression(None, Some(get_expression))
}

/// `generateResolvedIdentifier(nodeFactory, namespace, name)`: an identifier with its `ReferenceValue` prefilled
/// with a fresh public namespace named `namespace`.
pub fn generate_resolved_identifier(node_factory: &mut NodeFactory, namespace: &str, name: &str) -> NodeRef {
    let result = node_factory.identifier_intern(name, false);
    let namespaces = Namespaces::new();
    let namespace_value = new_public_namespace_value();
    namespace_value.set_name(namespace);
    namespaces.borrow_mut().push(namespace_value);
    let cx = node_factory.get_context();
    let reference_value = ReferenceValue::with_namespaces(&cx.borrow(), None, name, &namespaces);
    reference_value.set_is_attribute_identifier(false);
    result.borrow_mut().as_identifier_mut().unwrap().ref_ = Some(reference_value);
    result
}

/// `generateTypeExpression(nodeFactory, typeNamespace, typeName, intern)`.
pub fn generate_type_expression_ns(node_factory: &mut NodeFactory, type_namespace: &str, type_name: &str, intern: bool) -> NodeRef {
    let member_expression = generate_getter_selector_qualified(node_factory, type_namespace, type_name, intern);
    node_factory.type_expression(Some(member_expression), true, false, -1)
}

/// `generateTypeExpression(nodeFactory, type, intern)`.
pub fn generate_type_expression(node_factory: &mut NodeFactory, type_: &str, intern: bool) -> Option<NodeRef> {
    generate_type_expression_full(node_factory, type_, intern, false, -1)
}

/// `generateTypeExpression(nodeFactory, type, intern, position)`.
pub fn generate_type_expression_pos(node_factory: &mut NodeFactory, type_: &str, intern: bool, position: i32) -> Option<NodeRef> {
    generate_type_expression_full(node_factory, type_, intern, false, position)
}

/// `generateTypeExpression(nodeFactory, type, intern, includeAnyType)`.
pub fn generate_type_expression_any(node_factory: &mut NodeFactory, type_: &str, intern: bool, include_any_type: bool) -> Option<NodeRef> {
    generate_type_expression_full(node_factory, type_, intern, include_any_type, -1)
}

/// `generateTypeExpression(nodeFactory, type, intern, includeAnyType, position)`.
pub fn generate_type_expression_full(node_factory: &mut NodeFactory, type_: &str, intern: bool, include_any_type: bool, position: i32) -> Option<NodeRef> {
    if !include_any_type && type_ == "*" {
        return None;
    }
    let member_expression = if intern {
        match type_.find(LESS_THAN).filter(|&i| i > 0) {
            Some(less_than_index) => {
                let apply_type_expr = generate_apply_type_expr_at(node_factory, type_, less_than_index);
                node_factory.member_expression(None, Some(apply_type_expr))
            }
            None => match type_.rfind(DOT).filter(|&i| i > 0) {
                Some(dot_index) => generate_getter_selector_qualified(node_factory, &type_[..dot_index], &type_[dot_index + 1..], intern),
                None => generate_getter_selector(node_factory, type_, intern),
            },
        }
    } else {
        generate_getter_selector(node_factory, type_, intern)
    };
    Some(node_factory.type_expression(Some(member_expression), true, false, position))
}

/// `generateVariable(nodeFactory, name, intern)`: an untyped `var name`.
pub fn generate_variable_untyped(node_factory: &mut NodeFactory, name: &str, intern: bool) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, intern);
    let typed_identifier = node_factory.typed_identifier(Some(identifier), None);
    let variable_binding = node_factory.variable_binding(None, VAR_TOKEN, Some(typed_identifier), None);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(None, VAR_TOKEN, Some(list))
}

/// `generateVariable(nodeFactory, name, type, internType, rvalue)`.
pub fn generate_variable(node_factory: &mut NodeFactory, name: &str, type_: &str, intern_type: bool, rvalue: Option<NodeRef>) -> NodeRef {
    generate_variable_pos(node_factory, name, type_, intern_type, rvalue, -1)
}

/// `generateVariable(nodeFactory, name, type, internType, rvalue, position)`.
pub fn generate_variable_pos(node_factory: &mut NodeFactory, name: &str, type_: &str, intern_type: bool, rvalue: Option<NodeRef>, position: i32) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let type_expression = generate_type_expression_pos(node_factory, type_, intern_type, position);
    let typed_identifier = node_factory.typed_identifier(Some(identifier), type_expression);
    let variable_binding = node_factory.variable_binding(None, VAR_TOKEN, Some(typed_identifier), rvalue);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(None, VAR_TOKEN, Some(list))
}

/// `generateVariable(nodeFactory, name, typeNamespace, typeName, internType, rvalue)`.
pub fn generate_variable_ns(node_factory: &mut NodeFactory, name: &str, type_namespace: &str, type_name: &str, intern_type: bool, rvalue: Option<NodeRef>) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let type_expression = generate_type_expression_ns(node_factory, type_namespace, type_name, intern_type);
    let typed_identifier = node_factory.typed_identifier(Some(identifier), Some(type_expression));
    let variable_binding = node_factory.variable_binding(None, VAR_TOKEN, Some(typed_identifier), rvalue);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(None, VAR_TOKEN, Some(list))
}

/// `generateVariable(nodeFactory, attributeList, identifier, type, internType, rvalue)`.
pub fn generate_variable_attrs(node_factory: &mut NodeFactory, attribute_list: Option<NodeRef>, identifier: NodeRef, type_: &str, intern_type: bool, rvalue: Option<NodeRef>) -> NodeRef {
    let type_expression = generate_type_expression(node_factory, type_, intern_type);
    let typed_identifier = node_factory.typed_identifier(Some(identifier), type_expression);
    let variable_binding = node_factory.variable_binding(attribute_list.clone(), VAR_TOKEN, Some(typed_identifier), rvalue);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(attribute_list, VAR_TOKEN, Some(list))
}

/// `generateVariable(nodeFactory, attributeList, identifier, typeNamespace, typeName, internType, rvalue)`.
pub fn generate_variable_attrs_ns(node_factory: &mut NodeFactory, attribute_list: Option<NodeRef>, identifier: NodeRef, type_namespace: &str, type_name: &str, intern_type: bool, rvalue: Option<NodeRef>) -> NodeRef {
    let type_expression = generate_type_expression_ns(node_factory, type_namespace, type_name, intern_type);
    let typed_identifier = node_factory.typed_identifier(Some(identifier), Some(type_expression));
    let variable_binding = node_factory.variable_binding(attribute_list.clone(), VAR_TOKEN, Some(typed_identifier), rvalue);
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(attribute_list, VAR_TOKEN, Some(list))
}

/// `generateVariableNew(nodeFactory, name, type, position)`.
pub fn generate_variable_new(node_factory: &mut NodeFactory, name: &str, type_: &str, position: i32) -> NodeRef {
    generate_variable_new_args(node_factory, name, type_, None, position)
}

/// `generateVariableNew(nodeFactory, name, type, argumentList, position)`.
pub fn generate_variable_new_args(node_factory: &mut NodeFactory, name: &str, type_: &str, argument_list: Option<NodeRef>, position: i32) -> NodeRef {
    let identifier = node_factory.identifier_intern(name, false);
    let type_expression = generate_type_expression_pos(node_factory, type_, true, position);
    let typed_identifier = node_factory.typed_identifier(Some(identifier), type_expression);
    let initializer = match type_.find(LESS_THAN).filter(|&i| i > 0) {
        Some(less_than_index) => {
            let apply_type_expr = generate_apply_type_expr_at(node_factory, type_, less_than_index);
            let member_expression = node_factory.member_expression(None, Some(apply_type_expr));
            let call_expression = node_factory.call_expression(Some(member_expression), argument_list);
            call_expression.borrow_mut().as_call_expression_mut().unwrap().is_new = true;
            call_expression
        }
        None => {
            let type_identifier = generate_identifier(node_factory, type_, true);
            let call_expression = node_factory.call_expression(Some(type_identifier), argument_list);
            {
                let mut c = call_expression.borrow_mut();
                c.as_call_expression_mut().unwrap().is_new = true;
                c.selector_mut().unwrap().set_rvalue(false);
            }
            node_factory.member_expression(None, Some(call_expression))
        }
    };
    let variable_binding = node_factory.variable_binding(None, VAR_TOKEN, Some(typed_identifier), Some(initializer));
    let list = node_factory.list(None, Some(variable_binding));
    node_factory.variable_definition(None, VAR_TOKEN, Some(list))
}

/// `parse(context, configNamespaces, text, lineNumberOffset, emitDocInfo)`: a block of code such as a script block.
pub fn parse(context: &ContextRef, config_namespaces: &HashSet<String>, text: &str, line_number_offset: i32, emit_doc_info: bool) -> Vec<NodeRef> {
    parse_inner(context, config_namespaces, text, line_number_offset, emit_doc_info, false)
}

fn parse_inner(context: &ContextRef, config_namespaces: &HashSet<String>, text: &str, line_number_offset: i32, emit_doc_info: bool, is_expression: bool) -> Vec<NodeRef> {
    let mut result = Vec::new();
    let code_fragments_input_buffer = context.borrow().input.clone().expect("NullPointerException");
    let (origin, length) = {
        let b = code_fragments_input_buffer.borrow();
        (b.origin.clone().unwrap_or_default(), b.code_fragments.as_ref().expect("ClassCastException").get_length())
    };
    let offset_input_buffer = Rc::new(RefCell::new(InputBuffer::new_at(text, Some(origin.clone()), length, 0)));
    let mut parser = Parser::new_input(context, offset_input_buffer.clone(), &origin, emit_doc_info);
    parser.block_kind_stack.push(PACKAGE_TOKEN);
    parser.block_kind_stack.push(CLASS_TOKEN);
    parser.config_namespaces.borrow_mut().push(config_namespaces.clone());

    let original = toolkit::get_log_adapter();
    toolkit::set_log_adapter(Some(Rc::new(CodeFragmentLogAdapter::new(original.clone(), line_number_offset))));

    let statement_list = parser.parse_directives(None, None);
    parser.r#match(EOS_TOKEN);

    if let Some(statement_list) = &statement_list {
        let items: Vec<NodeRef> = statement_list.borrow().as_statement_list().map(|l| l.items.iter().flatten().cloned().collect()).unwrap_or_default();
        for node in items {
            let meta_data = if is_expression { node.borrow().as_meta_data().map(|m| m.data.clone()) } else { None };
            match meta_data {
                Some(data) => result.push(ExpressionStatementNode::new(Some(ListNode::new(None, data, -1)))),
                None => result.push(node),
            }
        }
    }

    toolkit::set_log_adapter(original);
    code_fragments_input_buffer.borrow_mut().code_fragments.as_mut().unwrap().add_code_fragment(crate::asc::util::jstr::utf16_len(text), offset_input_buffer, line_number_offset);
    context.borrow_mut().input = Some(code_fragments_input_buffer);
    result
}

/// `parseExpression(context, configNamespaces, text)`.
pub fn parse_expression_text(context: &ContextRef, config_namespaces: &HashSet<String>, text: &str) -> Vec<NodeRef> {
    parse_expression(context, config_namespaces, text, 0, false)
}

/// `parseExpression(context, configNamespaces, text, lineNumberOffset, emitDocInfo)`: an expression such as
/// `a = b`, where `[a, b]` is an array literal rather than metadata.
pub fn parse_expression(context: &ContextRef, config_namespaces: &HashSet<String>, text: &str, line_number_offset: i32, emit_doc_info: bool) -> Vec<NodeRef> {
    parse_inner(context, config_namespaces, text, line_number_offset, emit_doc_info, true)
}

/// `parseConfigVars(context, configNamespaces)`; `config_namespaces` gets the config namespaces it defines.
pub fn parse_config_vars(context: &ContextRef, config_namespaces: &mut HashSet<String>) -> Option<NodeRef> {
    let original_input_buffer = context.borrow().input.clone();
    let origin = original_input_buffer.as_ref().and_then(|b| b.borrow().origin.clone()).unwrap_or_default();
    let mut parser = Parser::new(context, "", &origin);
    parser.config_namespaces.borrow_mut().push(config_namespaces.clone());
    parser.config_namespaces.borrow_mut().last_mut().unwrap().insert(CONFIG.to_string());
    let result = parser.parse_config_values();
    if let Some(last) = parser.config_namespaces.borrow().last() {
        *config_namespaces = last.clone();
    }
    context.borrow_mut().input = original_input_buffer;
    result
}

/// `lineNumberToPosition(nodeFactory, lineNumber)`.
pub fn line_number_to_position(node_factory: &mut NodeFactory, line_number: i32) -> i32 {
    let cx = node_factory.get_context();
    let input = cx.borrow().input.clone().expect("NullPointerException");
    let mut b = input.borrow_mut();
    let fragments = b.code_fragments.as_mut().expect("ClassCastException");
    let position = fragments.get_length();
    fragments.add_line_number(line_number);
    position
}

/// `CodeFragmentLogAdapter`: offsets the line numbers ASC reports for a parsed code fragment by the fragment's line
/// in the MXML document.
pub struct CodeFragmentLogAdapter {
    original: Option<Rc<dyn LogAdapter>>,
    line_number_offset: i32,
}

impl CodeFragmentLogAdapter {
    pub fn new(original: Option<Rc<dyn LogAdapter>>, line_number_offset: i32) -> CodeFragmentLogAdapter {
        CodeFragmentLogAdapter { original, line_number_offset: line_number_offset - 1 }
    }
}

impl LogAdapter for CodeFragmentLogAdapter {
    fn log(&self, call: LogCall) {
        let off = self.line_number_offset;
        let call = match call {
            LogCall::PathLine { level, path, line, msg, code } => LogCall::PathLine { level, path, line: line + off, msg, code },
            LogCall::PathLineCol { level, path, line, msg, .. } => LogCall::PathLine { level, path, line: line + off, msg, code: None },
            LogCall::PathLineColSource { level, path, line, msg, code, .. } => LogCall::PathLine { level, path, line: line + off, msg, code },
            LogCall::Message { mut m, source } => {
                m.line += off;
                LogCall::Message { m, source }
            }
            other => other,
        };
        toolkit::forward_log(&self.original, call);
    }
}
