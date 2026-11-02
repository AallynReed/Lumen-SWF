//! Port of `flex2.compiler.as3.reflect.NodeMagic`: helpers that read (and a few that edit) ASC syntax trees.
//! `metaDataOnly` takes `LineNumberMap.get` as a function.

use std::collections::BTreeSet;

use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::NOTYPE;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::QName;
use crate::asc::parser::factory::NodeFactory;
use crate::asc::parser::node::{AttributeListNode, Node, NodeRef, StatementListNode};
use crate::asc::parser::tokens::{GET_TOKEN, SET_TOKEN};
use crate::asc::semantics::value::Value;
use crate::asc::util::context::ContextRef;
use crate::asc::util::jhash::{java_compare, JHashSet, JStr};
use crate::asc::util::multinames::Multinames;

pub const CONST: &str = "const";
pub const DYNAMIC: &str = "dynamic";
pub const FINAL: &str = "final";
pub const INTERNAL: &str = "internal";
pub const INTRINSIC: &str = "intrinsic";
pub const NATIVE: &str = "native";
pub const OVERRIDE: &str = "override";
pub const PRIVATE: &str = "private";
pub const PROTECTED: &str = "protected";
pub const PROTOTYPE: &str = "prototype";
pub const PUBLIC: &str = "public";
pub const STATIC: &str = "static";
pub const VIRTUAL: &str = "virtual";

fn nn(n: &Option<NodeRef>) -> NodeRef {
    n.clone().expect("NullPointerException")
}

pub fn get_variable_name(node: &NodeRef) -> String {
    let binding = get_variable_binding(node);
    let b = binding.borrow();
    let var = nn(&b.as_variable_binding().expect("ClassCastException").variable);
    let v = var.borrow();
    let id = nn(&v.as_typed_identifier().expect("ClassCastException").identifier);
    let name = id.borrow().identifier().expect("ClassCastException").name.clone();
    name
}

pub fn get_variable_binding(node: &NodeRef) -> NodeRef {
    let b = node.borrow();
    let list = nn(&b.as_variable_definition().expect("ClassCastException").list);
    let l = list.borrow();
    nn(&l.list().expect("ClassCastException").items[0])
}

pub fn set_variable_binding_name(node: &NodeRef, name: &str) {
    let b = node.borrow();
    let var = nn(&b.as_variable_binding().expect("ClassCastException").variable);
    let v = var.borrow();
    let id = nn(&v.as_typed_identifier().expect("ClassCastException").identifier);
    id.borrow_mut().identifier_mut().expect("ClassCastException").name = name.to_string();
}

/// `getVariableTypeName(VariableDefinitionNode)`.
pub fn get_variable_type_name(node: &NodeRef) -> String {
    get_variable_binding_type_name(&get_variable_binding(node))
}

/// `getVariableTypeName(VariableBindingNode)`.
pub fn get_variable_binding_type_name(node: &NodeRef) -> String {
    let b = node.borrow();
    let var = nn(&b.as_variable_binding().expect("ClassCastException").variable);
    let v = var.borrow();
    let type_ = v.as_typed_identifier().expect("ClassCastException").type_.clone();
    get_type_name(member_of_type(type_).as_ref())
}

/// The `MemberExpressionNode` of a type annotation (a `TypeExpressionNode`'s `expr`, or the annotation itself).
fn member_of_type(type_: Option<NodeRef>) -> Option<NodeRef> {
    let t = type_?;
    let te = t.borrow().as_type_expression().map(|te| te.expr.clone());
    match te {
        Some(e) => e,
        None => Some(t),
    }
}

fn type_name_from_selector(get_expression: &Node) -> String {
    let expr = nn(&get_expression.selector().expect("ClassCastException").expr);
    let e = expr.borrow();
    type_name_from_identifier(&e)
}

fn type_name_from_identifier(identifier: &Node) -> String {
    let mut result = identifier.identifier().expect("ClassCastException").name.clone();
    if let Some(q) = identifier.as_qualified_identifier() {
        if let Some(qual) = &q.qualifier {
            if let Some(ls) = qual.borrow().as_literal_string() {
                if !ls.value.is_empty() {
                    result = format!("{}.{}", ls.value, q.id.name);
                } else {
                    debug_assert!(false, "Empty LiteralStringNode");
                }
            } else {
                debug_assert!(false, "Empty QualifiedIdentifierNode");
            }
        } else {
            debug_assert!(false, "Empty QualifiedIdentifierNode");
        }
    }
    result
}

fn add_identifier(list: &mut Vec<String>, name: &str) {
    if name == OVERRIDE {
        list.insert(0, OVERRIDE.to_string());
    } else {
        list.push(name.to_string());
    }
}

/// `addImport(context, node, packageName)`.
pub fn add_import(context: &ContextRef, node: &NodeRef, package_name: &str) {
    let mut node_factory = NodeFactory::new(context);
    let mut package_identifiers: Option<NodeRef> = None;
    for token in package_name.split('.').filter(|t| !t.is_empty()) {
        let identifier = node_factory.identifier(token);
        package_identifiers = Some(node_factory.package_identifiers(package_identifiers, Some(identifier), true));
    }
    let package_name_node = node_factory.package_name(package_identifiers);
    let import_directive = node_factory.import_directive(None, Some(package_name_node), None, context);
    let pkgdef = node.borrow().class_definition().expect("ClassCastException").def.pkgdef.clone();
    import_directive.borrow_mut().as_import_directive_mut().expect("ClassCastException").def.pkgdef = pkgdef;
    let mut n = node.borrow_mut();
    let c = n.class_definition_mut().expect("ClassCastException");
    if c.statements.is_none() {
        c.statements = Some(StatementListNode::new(None));
    }
    let stmts = c.statements.clone().unwrap();
    drop(n);
    stmts.borrow_mut().as_statement_list_mut().expect("ClassCastException").items.insert(0, Some(import_directive));
}

fn check_for_identifier(list: &mut Vec<String>, attrs_node: &Node) {
    if let Some(m) = attrs_node.as_member_expression() {
        if let Some(sel) = &m.selector {
            let s = sel.borrow();
            if s.as_get_expression().is_some() {
                if let Some(expr) = &s.selector().unwrap().expr {
                    if let Some(id) = expr.borrow().identifier() {
                        add_identifier(list, &id.name);
                    }
                }
            }
        }
    } else if let Some(ls) = attrs_node.as_literal_string() {
        list.push(ls.value.clone());
    } else if let Some(id) = attrs_node.identifier() {
        add_identifier(list, &id.name);
    } else {
        debug_assert!(false, "Unexpected attribute node: {attrs_node}");
    }
}

/// `getMetaData(definition)`: the definition's metadata nodes.
pub fn get_meta_data(definition: &NodeRef) -> Vec<NodeRef> {
    let b = definition.borrow();
    let Some(md) = b.definition().and_then(|d| d.meta_data.clone()) else { return Vec::new() };
    let md = md.borrow();
    md.as_statement_list().map(|l| l.items.iter().flatten().cloned().collect()).unwrap_or_default()
}

fn meta_data_value_key(v: &Value) -> String {
    match v {
        Value::KeyValuePair(kv) => kv.key.clone(),
        Value::KeylessValue(k) => k.obj.clone(),
        _ => panic!("ClassCastException"),
    }
}

/// `getSortedMetaDataParamString(node)`: the values in a `TreeSet` by key, so equal keys collapse to the first.
pub fn get_sorted_meta_data_param_string(node: &NodeRef) -> String {
    let b = node.borrow();
    let md = b.meta_data().expect("ClassCastException");
    let mut parameters = String::new();
    if let Some(values) = md.get_values() {
        let mut params: Vec<Value> = Vec::new();
        for v in values {
            let k = meta_data_value_key(&v);
            match params.binary_search_by(|p| java_compare(&meta_data_value_key(p), &k)) {
                Ok(_) => {}
                Err(i) => params.insert(i, v),
            }
        }
        parameters.push('(');
        let n = params.len();
        for (i, v) in params.iter().enumerate() {
            match v {
                Value::KeyValuePair(pair) => {
                    parameters.push_str(&pair.key);
                    parameters.push_str("=\"");
                    parameters.push_str(&pair.obj);
                    parameters.push('"');
                }
                Value::KeylessValue(k) => {
                    parameters.push('"');
                    parameters.push_str(&k.obj);
                    parameters.push('"');
                }
                _ => {}
            }
            if i + 1 < n {
                parameters.push(',');
            }
        }
        parameters.push(')');
    }
    parameters
}

/// `getAttributes(DefinitionNode)`.
pub fn get_attributes(definition: &NodeRef) -> Vec<String> {
    let attrs = definition.borrow().definition().and_then(|d| d.attrs.clone());
    get_attribute_list_attributes(attrs.as_ref())
}

/// `getAttributes(AttributeListNode)`.
pub fn get_attribute_list_attributes(node: Option<&NodeRef>) -> Vec<String> {
    let mut result = Vec::new();
    if let Some(node) = node {
        let b = node.borrow();
        let attrs: &AttributeListNode = b.as_attribute_list().expect("ClassCastException");
        for attrs_node in attrs.items.iter().flatten() {
            let a = attrs_node.borrow();
            if let Some(list) = a.list() {
                for list_node in list.items.iter().flatten() {
                    check_for_identifier(&mut result, &list_node.borrow());
                }
            } else {
                check_for_identifier(&mut result, &a);
            }
        }
    }
    result
}

pub fn get_sorted_attribute_string(node: Option<&NodeRef>, delimiter: &str) -> String {
    set_to_string(&get_sorted_attributes(node), delimiter)
}

/// `getSortedAttributes(node)`, a `TreeSet<String>`.
pub fn get_sorted_attributes(node: Option<&NodeRef>) -> Vec<String> {
    let set: BTreeSet<JStr> = get_attribute_list_attributes(node).into_iter().map(JStr).collect();
    set.into_iter().map(|s| s.0).collect()
}

pub fn set_to_string(set: &[String], delimiter: &str) -> String {
    set.join(delimiter)
}

/// `getImports(multiNames)`.
pub fn get_imports(multi_names: &Multinames) -> JHashSet<String> {
    let mut result = JHashSet::new();
    for (class_name, namespaces) in multi_names.iter() {
        if namespaces.is_empty() {
            result.insert(class_name.to_string());
        } else {
            for object_value in namespaces.to_vec() {
                let package_name = object_value.to_string();
                if !package_name.is_empty() {
                    result.insert(format!("{package_name}.{class_name}"));
                } else {
                    result.insert(class_name.to_string());
                }
            }
        }
    }
    result
}

pub fn get_dotted_import_name(node: &NodeRef) -> String {
    let b = node.borrow();
    let imp = b.as_import_directive().expect("ClassCastException");
    let mut buf = String::new();
    if let Some(name) = &imp.name {
        let nb = name.borrow();
        if let Some(id) = nb.as_package_name().and_then(|p| p.id.clone()) {
            let ib = id.borrow();
            let list = &ib.as_package_identifiers().expect("ClassCastException").list;
            let n = list.len();
            for (i, item) in list.iter().enumerate() {
                buf.push_str(&nn(item).borrow().identifier().expect("ClassCastException").name);
                if i + 1 < n {
                    buf.push('.');
                }
            }
        }
    }
    buf
}

/// `getQName(qualifiedIdentifier)`.
pub fn get_qname(qualified_identifier: &NodeRef) -> QName {
    let b = qualified_identifier.borrow();
    let q = b.qualified_identifier().expect("ClassCastException");
    let mut namespace_uri: Option<String> = None;
    if let Some(qual) = &q.qualifier {
        let qb = qual.borrow();
        if let Some(m) = qb.as_member_expression() {
            if let Some(sel) = &m.selector {
                let s = sel.borrow();
                if s.as_get_expression().is_some() {
                    if let Some(expr) = &s.selector().unwrap().expr {
                        if let Some(id) = expr.borrow().identifier() {
                            namespace_uri = Some(id.name.clone());
                        }
                    }
                }
            }
        }
    }
    QName::new(namespace_uri.as_deref().unwrap_or(""), &q.id.name)
}

pub fn get_user_namespace(definition: &NodeRef) -> String {
    let mut result = String::new();
    for attribute in get_attributes(definition) {
        if !matches!(
            attribute.as_str(),
            CONST | DYNAMIC | FINAL | INTERNAL | INTRINSIC | NATIVE | OVERRIDE | PRIVATE | PROTECTED | PROTOTYPE | PUBLIC | STATIC | VIRTUAL
        ) {
            result = attribute;
            break;
        }
    }
    result
}

fn function_name_kind(node: &NodeRef) -> i32 {
    let b = node.borrow();
    let name = nn(&b.function_definition().expect("ClassCastException").name);
    let k = name.borrow().as_function_name().expect("ClassCastException").kind;
    k
}

pub fn function_is_getter(node: &NodeRef) -> bool {
    function_name_kind(node) == GET_TOKEN
}

pub fn function_is_setter(node: &NodeRef) -> bool {
    function_name_kind(node) == SET_TOKEN
}

fn function_signature(function: &NodeRef) -> NodeRef {
    let b = function.borrow();
    let fexpr = nn(&b.function_definition().expect("ClassCastException").fexpr);
    let f = fexpr.borrow();
    nn(&f.as_function_common().expect("ClassCastException").signature)
}

pub fn get_function_type_name(node: &NodeRef) -> String {
    let sig = function_signature(node);
    let result = sig.borrow().as_function_signature().expect("ClassCastException").result.clone();
    get_type_name(member_of_type(result).as_ref())
}

pub fn get_function_param_count(function: &NodeRef) -> i32 {
    let sig = function_signature(function);
    let params = sig.borrow().as_function_signature().expect("ClassCastException").parameter.clone();
    params.map_or(0, |p| p.borrow().as_parameter_list().expect("ClassCastException").items.len() as i32)
}

pub fn get_function_param_type_name(function: &NodeRef, pnum: i32) -> String {
    let sig = function_signature(function);
    let params = sig.borrow().as_function_signature().expect("ClassCastException").parameter.clone();
    let Some(params) = params else { return NOTYPE.to_string() };
    let pb = params.borrow();
    let items = &pb.as_parameter_list().expect("ClassCastException").items;
    if pnum as usize >= items.len() {
        return NOTYPE.to_string();
    }
    let param = nn(&items[pnum as usize]);
    let type_ = param.borrow().parameter().expect("ClassCastException").type_.clone();
    get_type_name(member_of_type(type_).as_ref())
}

pub fn get_function_name(function_definition: &NodeRef) -> Option<String> {
    let b = function_definition.borrow();
    let name = b.function_definition().expect("ClassCastException").name.clone()?;
    let nb = name.borrow();
    let id = nb.as_function_name().expect("ClassCastException").identifier.clone()?;
    let name = id.borrow().identifier().map(|i| i.name.clone());
    name
}

pub fn prefix_function_name(node: &NodeRef, prefix: &str) {
    let (name, fexpr) = {
        let b = node.borrow();
        let f = b.function_definition().expect("ClassCastException");
        (f.name.clone(), f.fexpr.clone())
    };
    if let Some(name) = name {
        let id = name.borrow().as_function_name().expect("ClassCastException").identifier.clone();
        if let Some(id) = id {
            let mut ib = id.borrow_mut();
            let i = ib.identifier_mut().expect("ClassCastException");
            i.name = format!("{prefix}{}", i.name);
        }
    }
    if let Some(fexpr) = fexpr {
        let mut fb = fexpr.borrow_mut();
        let f = fb.as_function_common_mut().expect("ClassCastException");
        f.internal_name = format!("{prefix}{}", f.internal_name);
    }
}

/// `getPackageName(ClassDefinitionNode)`.
pub fn get_class_package_name(node: &NodeRef) -> String {
    let pkgdef = node.borrow().class_definition().expect("ClassCastException").def.pkgdef.clone();
    get_package_name(pkgdef.as_ref())
}

/// `getPackageName(PackageDefinitionNode)`.
pub fn get_package_name(node: Option<&NodeRef>) -> String {
    let Some(p) = node else { return String::new() };
    let p = p.borrow();
    let Some(name) = p.as_package_definition().and_then(|p| p.name.clone()) else { return String::new() };
    let name = name.borrow();
    let Some(id) = name.as_package_name().and_then(|n| n.id.clone()) else { return String::new() };
    let mut id = id.borrow_mut();
    id.to_identifier_string().unwrap_or_default()
}

pub fn get_unqualified_function_name(node: &NodeRef) -> Option<String> {
    get_function_name(node)
}

pub fn get_unqualified_class_name(node: &NodeRef) -> String {
    let b = node.borrow();
    let name = nn(&b.class_definition().expect("ClassCastException").name);
    let n = name.borrow().identifier().expect("ClassCastException").name.clone();
    n
}

pub fn get_class_name(class_definition: &NodeRef) -> String {
    let cframe = class_definition.borrow().class_definition().expect("ClassCastException").cframe.clone();
    if let Some(cframe) = cframe {
        cframe.name().to_string()
    } else {
        let mut s = get_class_package_name(class_definition);
        if !s.is_empty() {
            s.push(':');
        }
        s.push_str(&get_unqualified_class_name(class_definition));
        s
    }
}

pub fn is_class_definition(n: &NodeRef) -> bool {
    let def = n.borrow().meta_data().expect("ClassCastException").def.clone();
    def.is_some_and(|d| d.borrow().class_definition().is_some())
}

pub fn retrieve_class_name(n: &NodeRef) -> Option<String> {
    if !is_class_definition(n) {
        return None;
    }
    let node = n.borrow().meta_data().unwrap().def.clone().unwrap();
    let cframe = node.borrow().class_definition().unwrap().cframe.clone();
    Some(match cframe {
        Some(cframe) => cframe.name().to_string().replace(':', "."),
        None => {
            let mut s = get_class_package_name(&node);
            if !s.is_empty() {
                s.push('.');
            }
            s.push_str(&get_unqualified_class_name(&node));
            s
        }
    })
}

pub fn normalize_class_name(class_name: Option<&str>) -> Option<String> {
    let class_name = class_name?;
    if !class_name.contains(':') {
        if let Some(dot) = class_name.rfind('.') {
            return Some(format!("{}:{}", &class_name[..dot], &class_name[dot + 1..]));
        }
    }
    Some(class_name.to_string())
}

/// `metaDataOnly(unit, map, beginLines, endLines)`; `map` is `LineNumberMap.get`.
pub fn meta_data_only(unit: &UnitRef, map: &dyn Fn(i32) -> i32, begin_lines: &[i32], end_lines: &[i32]) {
    let node = unit.borrow().get_syntax_tree().expect("NullPointerException");
    let (cx, stmts) = {
        let b = node.borrow();
        let p = b.program().expect("ClassCastException");
        (p.cx.clone().expect("NullPointerException"), p.statements.clone().expect("NullPointerException"))
    };
    let items: Vec<NodeRef> = stmts.borrow().as_statement_list().expect("ClassCastException").items.iter().flatten().cloned().collect();
    let input = cx.borrow().input.clone().expect("NullPointerException");
    for n in items {
        let (is_doc, is_meta, pos) = {
            let b = n.borrow();
            (b.as_doc_comment().is_some(), b.meta_data().is_some(), b.pos())
        };
        if is_doc || !is_meta {
            let ln = input.borrow_mut().get_ln_num(pos);
            let line = map(ln);
            let count = if line == 0 { 0 } else { begin_lines.len() };
            for j in 0..count {
                if line >= begin_lines[j] && line <= end_lines[j] {
                    let origin = input.borrow().origin.clone().unwrap_or_default();
                    toolkit::log(CompilerMessage::error("Only metadata is allowed."), Some(&origin), ln, -1, None);
                    break;
                }
            }
        }
    }
}

/// `getIdentifier(memberExpression)`.
pub fn get_identifier(member_expression: &NodeRef) -> Option<NodeRef> {
    let b = member_expression.borrow();
    let sel = b.as_member_expression().expect("ClassCastException").selector.clone()?;
    let s = sel.borrow();
    s.as_get_expression()?;
    let expr = s.selector().unwrap().expr.clone()?;
    let is_id = expr.borrow().identifier().is_some();
    is_id.then_some(expr)
}

fn get_type_name(member_expression: Option<&NodeRef>) -> String {
    let mut result = NOTYPE.to_string();
    if let Some(m) = member_expression {
        let b = m.borrow();
        let sel = nn(&b.as_member_expression().expect("ClassCastException").selector);
        let s = sel.borrow();
        if s.as_get_expression().is_some() {
            result = type_name_from_selector(&s);
        } else if let Some(apply_type) = s.as_apply_type_expr() {
            let expr = nn(&apply_type.sel.expr);
            let e = expr.borrow();
            if e.identifier().is_some() {
                result = type_name_from_identifier(&e);
                if let Some(type_args) = &apply_type.type_args {
                    result.push_str(".<");
                    let tb = type_args.borrow();
                    let items = &tb.list().expect("ClassCastException").items;
                    let n = items.len();
                    for (i, node) in items.iter().enumerate() {
                        let node = nn(node);
                        let te = node.borrow().as_type_expression().map(|t| t.expr.clone());
                        if let Some(expr) = te {
                            result.push_str(&get_type_name(expr.as_ref()));
                        }
                        if i + 1 < n {
                            result.push_str(", ");
                        }
                    }
                    result.push('>');
                }
            } else {
                debug_assert!(false, "Unexpected ApplyTypeExprNode expr type");
            }
        } else {
            debug_assert!(false, "Unexpected MemberExpressionNode selector type");
        }
    }
    result
}

/// `lookupType(variableBinding)`: the short type name.
pub fn lookup_type(variable_binding: &NodeRef) -> Option<String> {
    let b = variable_binding.borrow();
    let vb = b.as_variable_binding().expect("ClassCastException");
    let mut member_expression = None;
    if let Some(var) = &vb.variable {
        let v = var.borrow();
        if let Some(t) = v.as_typed_identifier().and_then(|t| t.type_.clone()) {
            let te = t.borrow().as_type_expression().map(|te| te.expr.clone());
            match te {
                Some(e) => member_expression = e,
                None => {
                    if t.borrow().as_member_expression().is_some() {
                        member_expression = Some(t.clone());
                    }
                }
            }
        }
    }
    let m = member_expression?;
    let mb = m.borrow();
    let sel = mb.as_member_expression()?.selector.clone()?;
    let sb = sel.borrow();
    let get = sb.as_get_expression()?;
    let id = get.sel.get_identifier()?;
    let name = id.borrow().identifier().map(|i| i.name.clone());
    name
}

pub fn remove_meta_data(definition_node: &NodeRef, id: &str) {
    let md = definition_node.borrow().definition().and_then(|d| d.meta_data.clone());
    let Some(md) = md else { return };
    let mut mb = md.borrow_mut();
    let Some(list) = mb.as_statement_list_mut() else { return };
    list.items.retain(|item| {
        let item = nn(item);
        let b = item.borrow();
        let m = b.meta_data().expect("ClassCastException");
        m.get_id().as_deref() != Some(id)
    });
}
