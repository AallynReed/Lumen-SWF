//! Ports of `flex2.compiler.as3.SignatureExtension`, `SignatureEvaluator` and `SignatureRules`: the class signature
//! of a source, whose Adler32 checksum incremental compilation compares to decide whether dependents recompile.
//! `SignatureEvaluator` implements the ASC `Evaluator` directly rather than through `evaluator_adapter`, since it
//! depends on `checkFeature` running before each node's `evaluate`.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::extension::Extension;
use super::reflect::node_magic;
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::symbol_table::{SymbolTable, NOTYPE};
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage};
use crate::asc::flex2::compiler::util::mime_mappings;
use crate::asc::parser::evaluator::{evaluate, Evaluator};
use crate::asc::parser::meta_data_evaluator::MetaDataEvaluator;
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::parser::tokens::{CONST_TOKEN, VAR_TOKEN};
use crate::asc::semantics::value::Value;
use crate::asc::util::context::ContextRef;
use crate::asc::util::jhash::{java_compare, JStr};

/// `SignatureEvaluator.NEWLINE`, `System.getProperty("line.separator")`.
pub const NEWLINE: &str = if cfg!(windows) { "\r\n" } else { "\n" };

pub const DEFAULT_SIG_DIR: &str = "generated-signatures";
pub const WARNING_ATTRIBUTE: &str = "SignatureExtension.warning";

/// `SignatureExtension.debug`.
const DEBUG: bool = true;

/// `SignatureRules`.
pub struct SignatureRules {
    pub keep_classes: bool,
    pub keep_interfaces: bool,
    pub keep_fun_scope_user: bool,
    pub keep_fun_scope_internal: bool,
    pub keep_fun_scope_private: bool,
    pub keep_fun_scope_protected: bool,
    pub keep_fun_scope_public: bool,
    pub keep_fun_param_initializer: bool,
    pub keep_var_scope_user: bool,
    pub keep_var_scope_internal: bool,
    pub keep_var_scope_private: bool,
    pub keep_var_scope_protected: bool,
    pub keep_var_scope_public: bool,
    pub keep_imports: bool,
    pub keep_use_namespace: bool,
    pub keep_metadata: bool,
}

impl Default for SignatureRules {
    fn default() -> Self {
        SignatureRules {
            keep_classes: true,
            keep_interfaces: true,
            keep_fun_scope_user: true,
            keep_fun_scope_internal: true,
            keep_fun_scope_private: false,
            keep_fun_scope_protected: true,
            keep_fun_scope_public: true,
            keep_fun_param_initializer: false,
            keep_var_scope_user: true,
            keep_var_scope_internal: true,
            keep_var_scope_private: false,
            keep_var_scope_protected: true,
            keep_var_scope_public: true,
            keep_imports: true,
            keep_use_namespace: true,
            keep_metadata: true,
        }
    }
}

/// `AttributeInfo`, computed afresh where Java consults its `AttributeInfoCache`.
struct AttributeInfo {
    is_internal: bool,
    is_private: bool,
    is_public: bool,
    is_protected: bool,
    is_user: bool,
}

impl AttributeInfo {
    fn new(node: &NodeRef) -> AttributeInfo {
        let attrs = node.borrow().definition().expect("ClassCastException").attrs.clone();
        match attrs {
            None => AttributeInfo { is_internal: true, is_private: false, is_public: false, is_protected: false, is_user: false },
            Some(attrs) => {
                let a = attrs.borrow();
                let is_public = a.has_attribute(node_magic::PUBLIC);
                let is_private = a.has_attribute(node_magic::PRIVATE);
                let is_protected = a.has_attribute(node_magic::PROTECTED);
                let is_user = !node_magic::get_user_namespace(node).is_empty();
                let is_internal = a.has_attribute(node_magic::INTERNAL) || !(is_public || is_private || is_protected || is_user);
                AttributeInfo { is_internal, is_private, is_public, is_protected, is_user }
            }
        }
    }
}

type Definitions = BTreeMap<JStr, Option<BTreeSet<JStr>>>;

fn nn(n: &Option<NodeRef>) -> NodeRef {
    n.clone().expect("NullPointerException")
}

/// `SignatureEvaluator`.
pub struct SignatureEvaluator {
    out: String,
    indent: i32,
    signature_rules: SignatureRules,
    human_readable: bool,
    inside_package: bool,
    indent_cache: Option<String>,
    /// Never updated, as in Java.
    last_indent: i32,
    current_package: Option<NodeRef>,
}

impl SignatureEvaluator {
    pub fn new(suggested_buffer_size: usize, human_readable: bool) -> SignatureEvaluator {
        Self::with_rules(suggested_buffer_size, human_readable, SignatureRules::default())
    }

    pub fn with_rules(suggested_buffer_size: usize, human_readable: bool, signature_rules: SignatureRules) -> SignatureEvaluator {
        SignatureEvaluator {
            out: String::with_capacity(suggested_buffer_size),
            indent: 0,
            signature_rules,
            human_readable,
            inside_package: false,
            indent_cache: None,
            last_indent: 0,
            current_package: None,
        }
    }

    pub fn get_signature(&self) -> String {
        self.out.clone()
    }

    /// `indent()`, with Java's cache that only refreshes away from indent 0 (`null` before the first refresh).
    fn indent(&mut self) -> String {
        if self.last_indent != self.indent {
            self.indent_cache = Some("    ".repeat(self.indent.max(0) as usize));
        }
        self.indent_cache.clone().unwrap_or_else(|| "null".to_string())
    }

    fn put_indent(&mut self) {
        let s = self.indent();
        self.out.push_str(&s);
    }

    fn hr(&mut self, human_readable: &str, compact: &str) {
        let s = if self.human_readable { human_readable } else { compact };
        self.out.push_str(s);
    }

    fn separator(&mut self) {
        self.hr(", ", " ");
    }

    /// The end of a class or interface header.
    fn close_header(&mut self) {
        if self.human_readable {
            self.out.push_str(NEWLINE);
            self.put_indent();
            self.out.push('{');
            self.out.push_str(NEWLINE);
        }
    }

    /// A class or interface body and its closing brace.
    fn body(&mut self, cx: &ContextRef, statements: Option<NodeRef>) {
        if let Some(statements) = statements {
            self.indent += 1;
            evaluate(&statements, cx, self);
            self.indent -= 1;
        }
        if self.human_readable {
            self.put_indent();
            self.out.push('}');
            self.out.push_str(NEWLINE);
        }
    }

    /// `out` swapped for a fresh builder while `f` runs.
    fn capture(&mut self, f: impl FnOnce(&mut Self)) -> String {
        let original = std::mem::take(&mut self.out);
        f(self);
        std::mem::replace(&mut self.out, original)
    }

    fn check_feature_of(&self, node: Option<&NodeRef>) -> bool {
        let Some(node) = node else { return true };
        let b = node.borrow();
        if b.definition().is_some() {
            let att = AttributeInfo::new(node);
            let r = &self.signature_rules;
            match &b.kind {
                Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => {
                    !((att.is_public && !r.keep_fun_scope_public)
                        || (att.is_private && !r.keep_fun_scope_private)
                        || (att.is_protected && !r.keep_fun_scope_protected)
                        || (att.is_user && !r.keep_fun_scope_user)
                        || (att.is_internal && !r.keep_fun_scope_internal))
                }
                Kind::VariableDefinition(_) => {
                    !((att.is_public && !r.keep_var_scope_public)
                        || (att.is_private && !r.keep_var_scope_private)
                        || (att.is_protected && !r.keep_var_scope_protected)
                        || (att.is_user && !r.keep_var_scope_user)
                        || (att.is_internal && !r.keep_var_scope_internal))
                }
                Kind::ImportDirective(_) => r.keep_imports,
                _ if b.class_definition().is_some() => r.keep_classes,
                Kind::UseDirective(_) => r.keep_use_namespace,
                _ => true,
            }
        } else if let Some(md) = b.meta_data() {
            let def = md.def.clone();
            drop(b);
            self.signature_rules.keep_metadata && self.check_feature_of(def.as_ref())
        } else {
            true
        }
    }

    /// The sorted attributes of a function or variable definition, `internal` added when no access specifier is.
    fn definition_attributes(node: &NodeRef, attrs: Option<&NodeRef>) -> String {
        let mut set = node_magic::get_sorted_attributes(attrs);
        if attrs.is_none() || AttributeInfo::new(node).is_internal {
            if let Err(i) = set.binary_search_by(|a| java_compare(a, node_magic::INTERNAL)) {
                set.insert(i, node_magic::INTERNAL.to_string());
            }
        }
        node_magic::set_to_string(&set, " ")
    }

    /// `evaluateSorted(cx, list)`.
    pub fn evaluate_sorted(&mut self, cx: &ContextRef, list: &NodeRef) -> Option<Value> {
        let items: Vec<NodeRef> = list.borrow().list().expect("ClassCastException").items.iter().flatten().cloned().collect();
        let mut sorted = BTreeSet::new();
        for node in &items {
            let s = self.capture(|e| {
                evaluate(node, cx, e);
            });
            sorted.insert(JStr(s));
        }
        let n = sorted.len();
        for (i, s) in sorted.into_iter().enumerate() {
            self.out.push_str(&s.0);
            if i + 1 < n {
                self.separator();
            }
        }
        None
    }

    fn evaluate_items(&mut self, cx: &ContextRef, items: Vec<Option<NodeRef>>) {
        let n = items.len();
        for (i, item) in items.iter().enumerate() {
            evaluate(&nn(item), cx, self);
            if i + 1 < n {
                self.separator();
            }
        }
    }

    /// `evaluate(cx, QualifiedIdentifierNode)` on a qualified identifier or expression.
    fn qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) {
        let (qualifier, name) = {
            let b = node.borrow();
            let q = b.qualified_identifier().expect("ClassCastException");
            (q.qualifier.clone(), q.id.name.clone())
        };
        if let Some(q) = qualifier {
            evaluate(&q, cx, self);
            self.out.push_str("::");
        }
        self.out.push_str(&name);
    }

    fn put_definition(&mut self, declarations: &mut Definitions, cx: &ContextRef, definition: &NodeRef) {
        let declaration = self.capture(|e| {
            evaluate(definition, cx, e);
        });
        let meta = definition.borrow().definition().expect("ClassCastException").meta_data.clone();
        let mut meta_data = None;
        if let Some(meta) = meta {
            let items: Vec<NodeRef> = meta.borrow().as_statement_list().expect("ClassCastException").items.iter().flatten().cloned().collect();
            let mut set = BTreeSet::new();
            for n in &items {
                let s = self.capture(|e| {
                    evaluate(n, cx, e);
                });
                set.insert(JStr(s));
            }
            meta_data = Some(set);
        }
        declarations.insert(JStr(declaration), meta_data);
    }

    fn append_statements(&mut self, statements: Option<BTreeSet<JStr>>) {
        for s in statements.into_iter().flatten() {
            self.out.push_str(&s.0);
        }
    }

    fn append_definitions(&mut self, definitions: Option<Definitions>) {
        for (declaration, meta_data) in definitions.into_iter().flatten() {
            for m in meta_data.into_iter().flatten() {
                self.out.push_str(&m.0);
            }
            self.out.push_str(&declaration.0);
        }
    }

    fn append_sorted(&mut self, imports: Option<BTreeSet<JStr>>, use_namespaces: Option<BTreeSet<JStr>>, variables: Option<Definitions>, functions: Option<Definitions>, classes: Option<Definitions>) {
        self.append_statements(imports);
        self.append_statements(use_namespaces);
        self.append_definitions(variables);
        self.append_definitions(functions);
        self.append_definitions(classes);
    }
}

enum Statement {
    Import,
    Namespace,
    Class,
    Function,
    Variable,
    Package,
    MetaData,
    Other,
}

fn statement_kind(node: &NodeRef) -> Statement {
    let b = node.borrow();
    match &b.kind {
        Kind::ImportDirective(_) => Statement::Import,
        _ if b.namespace_definition().is_some() => Statement::Namespace,
        _ if b.class_definition().is_some() => Statement::Class,
        _ if b.function_definition().is_some() => Statement::Function,
        Kind::VariableDefinition(_) => Statement::Variable,
        Kind::PackageDefinition(_) => Statement::Package,
        _ if b.meta_data().is_some() => Statement::MetaData,
        _ => Statement::Other,
    }
}

impl Evaluator for SignatureEvaluator {
    fn check_feature(&mut self, _cx: &ContextRef, node: &NodeRef) -> bool {
        self.check_feature_of(Some(node))
    }

    fn evaluate_program(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, main_package) = {
            let b = node.borrow();
            let p = b.program().expect("ClassCastException");
            (p.cx.clone().unwrap_or_else(|| cx.clone()), p.pkgdefs.first().cloned())
        };
        if let Some(main_package) = main_package {
            let statements = main_package.borrow().as_package_definition().expect("ClassCastException").statements.clone();
            if let Some(statements) = statements {
                self.indent += 1;
                evaluate(&statements, &cx, self);
                self.indent -= 1;
            }
        }
        None
    }

    fn evaluate_class_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, attrs, baseclass, interfaces, statements) = {
            let b = node.borrow();
            let c = b.class_definition().expect("ClassCastException");
            (c.cx.clone().unwrap_or_else(|| cx.clone()), c.def.attrs.clone(), c.baseclass.clone(), c.interfaces.clone(), c.statements.clone())
        };
        if self.human_readable {
            self.put_indent();
        }
        if let Some(attrs) = attrs {
            evaluate(&attrs, &cx, self);
        }
        self.hr("class ", "C");
        self.out.push_str(&node_magic::get_unqualified_class_name(node));
        if let Some(baseclass) = baseclass {
            self.hr(" extends ", " E");
            evaluate(&baseclass, &cx, self);
        }
        if let Some(interfaces) = interfaces {
            self.hr(" implements ", " I");
            self.evaluate_sorted(&cx, &interfaces);
        }
        self.close_header();
        self.body(&cx, statements);
        None
    }

    fn evaluate_interface_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (attrs, interfaces, statements) = {
            let b = node.borrow();
            let c = b.class_definition().expect("ClassCastException");
            (c.def.attrs.clone(), c.interfaces.clone(), c.statements.clone())
        };
        if self.human_readable {
            self.put_indent();
        }
        if let Some(attrs) = attrs {
            evaluate(&attrs, cx, self);
        }
        self.hr("interface ", "I");
        self.out.push_str(&node_magic::get_unqualified_class_name(node));
        if let Some(interfaces) = interfaces {
            self.hr(" extends ", " E");
            self.evaluate_sorted(cx, &interfaces);
        }
        self.close_header();
        self.body(cx, statements);
        None
    }

    fn evaluate_function_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (cx, attrs, fexpr) = {
            let b = node.borrow();
            let f = b.function_definition().expect("ClassCastException");
            (f.cx.clone().unwrap_or_else(|| cx.clone()), f.def.attrs.clone(), nn(&f.fexpr))
        };
        let attributes = Self::definition_attributes(node, attrs.as_ref());
        if self.human_readable {
            self.put_indent();
        }
        self.out.push_str(&attributes);
        self.hr(" function ", "F");
        if node_magic::function_is_getter(node) {
            self.hr("get ", "G ");
        } else if node_magic::function_is_setter(node) {
            self.hr("set ", "S ");
        }
        self.out.push_str(node_magic::get_unqualified_function_name(node).as_deref().unwrap_or("null"));

        let signature = nn(&fexpr.borrow().as_function_common().expect("ClassCastException").signature);
        let (parameter, result, void_anno, no_anno) = {
            let b = signature.borrow();
            let s = b.as_function_signature().expect("ClassCastException");
            (s.parameter.clone(), s.result.clone(), s.void_anno, s.no_anno)
        };
        if self.human_readable {
            self.out.push('(');
        }
        if node_magic::get_function_param_count(node) > 0 {
            let items = nn(&parameter).borrow().as_parameter_list().expect("ClassCastException").items.clone();
            let n = items.len();
            for (i, param) in items.iter().enumerate() {
                let param = nn(param);
                let (rest, type_, param_no_anno, init) = {
                    let b = param.borrow();
                    let p = b.parameter().expect("ClassCastException");
                    (b.as_rest_parameter().is_some(), p.type_.clone(), p.no_anno, p.init.clone())
                };
                if rest {
                    self.out.push_str("...");
                } else {
                    if let Some(type_) = type_ {
                        evaluate(&type_, &cx, self);
                    } else if !param_no_anno {
                        self.out.push_str(NOTYPE);
                    }
                    if let Some(init) = init {
                        if self.signature_rules.keep_fun_param_initializer {
                            self.out.push_str("=<");
                            evaluate(&init, &cx, self);
                            self.out.push('>');
                        } else {
                            self.out.push_str("=...");
                        }
                    }
                    if i + 1 < n {
                        self.out.push_str(", ");
                    }
                }
            }
        }
        if self.human_readable {
            self.out.push(')');
        }

        if void_anno || !no_anno {
            if self.human_readable {
                self.out.push(':');
            }
            match result {
                _ if void_anno => self.out.push_str("void"),
                Some(result) => {
                    evaluate(&result, &cx, self);
                }
                None => self.out.push_str(NOTYPE),
            }
        }
        if self.human_readable {
            self.out.push_str(NEWLINE);
        }
        None
    }

    fn evaluate_variable_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (attrs, kind, list) = {
            let b = node.borrow();
            let v = b.as_variable_definition().expect("ClassCastException");
            (v.def.attrs.clone(), v.kind, nn(&v.list))
        };
        let attributes = Self::definition_attributes(node, attrs.as_ref());
        let kind = match (kind, self.human_readable) {
            (CONST_TOKEN, true) => "const ",
            (VAR_TOKEN, true) => "var ",
            (CONST_TOKEN, false) => "C",
            (VAR_TOKEN, false) => "V",
            _ => "null",
        };
        let items = list.borrow().list().expect("ClassCastException").items.clone();
        for item in &items {
            let item = nn(item);
            let (variable, initializer) = {
                let b = item.borrow();
                let vb = b.as_variable_binding().expect("ClassCastException");
                (nn(&vb.variable), vb.initializer.is_some())
            };
            let (name, type_, no_anno) = {
                let b = variable.borrow();
                let t = b.as_typed_identifier().expect("ClassCastException");
                let name = nn(&t.identifier).borrow().identifier().expect("ClassCastException").name.clone();
                (name, t.type_.clone(), t.no_anno)
            };
            if self.human_readable {
                self.put_indent();
            }
            self.out.push_str(&attributes);
            if self.human_readable {
                self.out.push(' ');
            }
            self.out.push_str(kind);
            self.out.push_str(&name);
            if !no_anno {
                if self.human_readable {
                    self.out.push(':');
                }
                match type_ {
                    Some(type_) => {
                        evaluate(&type_, cx, self);
                    }
                    None => self.out.push_str(NOTYPE),
                }
            }
            if initializer {
                self.out.push_str("=...");
            }
            if self.human_readable {
                self.out.push_str(NEWLINE);
            }
        }
        None
    }

    fn evaluate_import_directive(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if self.human_readable {
            self.put_indent();
            self.out.push_str("import ");
        } else {
            self.out.push_str("I ");
        }
        self.out.push_str(&node_magic::get_dotted_import_name(node));
        if self.human_readable {
            self.out.push_str(NEWLINE);
        }
        None
    }

    fn evaluate_namespace_definition(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (attrs, name, value) = {
            let b = node.borrow();
            let n = b.namespace_definition().expect("ClassCastException");
            (n.def.attrs.clone(), n.name.clone(), n.value.clone())
        };
        if self.human_readable {
            self.put_indent();
        }
        if let Some(attrs) = attrs {
            evaluate(&attrs, cx, self);
        }
        self.hr("namespace ", "N");
        if let Some(name) = name {
            evaluate(&name, cx, self);
        }
        if let Some(value) = value {
            self.out.push('=');
            evaluate(&value, cx, self);
        }
        if self.human_readable {
            self.out.push_str(NEWLINE);
        }
        None
    }

    fn evaluate_package_definition(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if self.current_package.is_none() {
            self.current_package = Some(node.clone());
            self.hr("package ", "P");
            self.out.push_str(&node_magic::get_package_name(Some(node)));
            if self.human_readable {
                self.out.push_str(NEWLINE);
                self.out.push('{');
                self.out.push_str(NEWLINE);
            }
        } else {
            self.current_package = None;
            if self.human_readable {
                self.out.push('}');
                self.out.push_str(NEWLINE);
            }
        }
        None
    }

    fn evaluate_use_directive(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        if self.human_readable {
            self.put_indent();
            self.out.push_str("use namespace ");
        } else {
            self.out.push('U');
        }
        let expr = node.borrow().as_use_directive().expect("ClassCastException").expr.clone();
        if let Some(expr) = expr {
            if expr.borrow().list().is_some() {
                self.evaluate_sorted(cx, &expr);
            } else {
                evaluate(&expr, cx, self);
            }
        }
        if self.human_readable {
            self.out.push_str(NEWLINE);
        }
        None
    }

    fn evaluate_meta_data(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let has_data = node.borrow().meta_data().expect("ClassCastException").data.is_some();
        if has_data {
            evaluate(node, cx, &mut MetaDataEvaluator::new());
        }
        if self.human_readable {
            self.put_indent();
            self.out.push('[');
        }
        let id = node.borrow().meta_data().unwrap().get_id();
        self.out.push_str(id.as_deref().unwrap_or("null"));
        self.out.push_str(&node_magic::get_sorted_meta_data_param_string(node));
        if self.human_readable {
            self.out.push(']');
            self.out.push_str(NEWLINE);
        }
        None
    }

    fn evaluate_argument_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().as_argument_list().expect("ClassCastException").items.clone();
        self.evaluate_items(cx, items);
        None
    }

    fn evaluate_list(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let items = node.borrow().list().expect("ClassCastException").items.clone();
        self.evaluate_items(cx, items);
        None
    }

    fn evaluate_get_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = node.borrow().selector().expect("ClassCastException").expr.clone();
        if let Some(expr) = expr {
            if expr.borrow().as_argument_list().is_some() {
                self.out.push('[');
                evaluate(&expr, cx, self);
                self.out.push(']');
            } else {
                evaluate(&expr, cx, self);
            }
        }
        None
    }

    fn evaluate_qualified_identifier(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.qualified_identifier(cx, node);
        None
    }

    fn evaluate_qualified_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.out.push('@');
        self.qualified_identifier(cx, node);
        let expr = node.borrow().as_qualified_expression().expect("ClassCastException").expr.clone();
        if let Some(expr) = expr {
            self.out.push('[');
            evaluate(&expr, cx, self);
            self.out.push(']');
        }
        None
    }

    fn evaluate_member_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (base, selector) = {
            let b = node.borrow();
            let m = b.as_member_expression().expect("ClassCastException");
            (m.base.clone(), m.selector.clone())
        };
        if let Some(base) = base {
            evaluate(&base, cx, self);
            let dotted = selector.as_ref().is_some_and(|s| {
                let s = s.borrow();
                s.as_get_expression().is_some_and(|g| !g.sel.expr.as_ref().is_some_and(|e| e.borrow().as_argument_list().is_some()))
            });
            if dotted {
                self.out.push('.');
            }
        }
        if let Some(selector) = selector {
            evaluate(&selector, cx, self);
        }
        None
    }

    fn evaluate_type_expression(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let expr = nn(&node.borrow().as_type_expression().expect("ClassCastException").expr);
        evaluate(&expr, cx, self)
    }

    fn evaluate_attribute_list(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let attrs = node_magic::get_sorted_attribute_string(Some(node), " ");
        if !attrs.is_empty() {
            self.out.push_str(&attrs);
            self.out.push(' ');
        }
        None
    }

    fn evaluate_apply_type_expr(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (expr, type_args) = {
            let b = node.borrow();
            let a = b.as_apply_type_expr().expect("ClassCastException");
            (a.sel.expr.clone(), a.type_args.clone())
        };
        if let Some(expr) = expr {
            evaluate(&expr, cx, self);
        }
        self.out.push_str(".<");
        if let Some(type_args) = type_args {
            evaluate(&type_args, cx, self);
        }
        self.out.push('>');
        None
    }

    fn evaluate_identifier(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let name = node.borrow().identifier().expect("ClassCastException").name.clone();
        self.out.push_str(&name);
        None
    }

    fn evaluate_literal_array(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.out.push('[');
        let elementlist = node.borrow().as_literal_array().expect("ClassCastException").elementlist.clone();
        if let Some(elementlist) = elementlist {
            evaluate(&elementlist, cx, self);
        }
        self.out.push(']');
        None
    }

    fn evaluate_literal_boolean(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_boolean().expect("ClassCastException").value;
        self.out.push_str(if value { "true" } else { "false" });
        None
    }

    fn evaluate_literal_field(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (name, value) = {
            let b = node.borrow();
            let f = b.as_literal_field().expect("ClassCastException");
            (f.name.clone(), f.value.clone())
        };
        if let Some(name) = name {
            evaluate(&name, cx, self);
        }
        if let Some(value) = value {
            self.out.push(':');
            evaluate(&value, cx, self);
        }
        None
    }

    fn evaluate_literal_null(&mut self, _cx: &ContextRef, _node: &NodeRef) -> Option<Value> {
        self.out.push_str("null");
        None
    }

    fn evaluate_literal_number(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_number().expect("ClassCastException").value.clone();
        self.out.push_str(&value);
        None
    }

    fn evaluate_literal_object(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        self.out.push('{');
        let fieldlist = node.borrow().as_literal_object().expect("ClassCastException").fieldlist.clone();
        if let Some(fieldlist) = fieldlist {
            evaluate(&fieldlist, cx, self);
        }
        self.out.push('}');
        None
    }

    fn evaluate_literal_reg_exp(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_reg_exp().expect("ClassCastException").value.clone();
        self.out.push_str(&value);
        None
    }

    fn evaluate_literal_string(&mut self, _cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let value = node.borrow().as_literal_string().expect("ClassCastException").value.clone();
        self.out.push_str(&value);
        None
    }

    /// `EvaluatorAdapter.evaluate(cx, LiteralXMLNode)`.
    fn evaluate_literal_xml(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let list = node.borrow().as_literal_xml().expect("ClassCastException").list.clone();
        if let Some(list) = list {
            evaluate(&list, cx, self);
        }
        None
    }

    /// `EvaluatorAdapter.evaluate(cx, LiteralVectorNode)`.
    fn evaluate_literal_vector(&mut self, cx: &ContextRef, node: &NodeRef) -> Option<Value> {
        let (type_, elementlist) = {
            let b = node.borrow();
            let v = b.as_literal_vector().expect("ClassCastException");
            (nn(&v.type_), v.elementlist.clone())
        };
        evaluate(&type_, cx, self);
        if let Some(elementlist) = elementlist {
            evaluate(&elementlist, cx, self);
        }
        None
    }

    fn evaluate_statement_list(&mut self, cx: &ContextRef, statement_list: &NodeRef) -> Option<Value> {
        let mut imports: Option<BTreeSet<JStr>> = None;
        let mut use_namespaces: Option<BTreeSet<JStr>> = None;
        let mut variables: Option<Definitions> = None;
        let mut functions: Option<Definitions> = None;
        // Interfaces are ClassDefinitionNodes, so Java's interface map never gets an entry.
        let mut classes: Option<Definitions> = None;

        let mut i = 0;
        loop {
            let node = {
                let b = statement_list.borrow();
                let items = &b.as_statement_list().expect("ClassCastException").items;
                if i >= items.len() {
                    break;
                }
                items[i].clone()
            };
            i += 1;
            let Some(node) = node else { continue };
            match (statement_kind(&node), self.inside_package) {
                (Statement::Import, true) => {
                    let s = self.capture(|e| {
                        evaluate(&node, cx, e);
                    });
                    imports.get_or_insert_with(BTreeSet::new).insert(JStr(s));
                }
                (Statement::Namespace, true) => {
                    let s = self.capture(|e| {
                        evaluate(&node, cx, e);
                    });
                    use_namespaces.get_or_insert_with(BTreeSet::new).insert(JStr(s));
                }
                (Statement::Class, true) => self.put_definition(classes.get_or_insert_with(BTreeMap::new), cx, &node),
                (Statement::Function, true) => self.put_definition(functions.get_or_insert_with(BTreeMap::new), cx, &node),
                (Statement::Variable, true) => self.put_definition(variables.get_or_insert_with(BTreeMap::new), cx, &node),
                (Statement::Package, true) => {
                    self.append_sorted(imports.take(), use_namespaces.take(), variables.take(), functions.take(), classes.take());
                    evaluate(&node, cx, self);
                    self.inside_package = false;
                }
                (Statement::Package, false) => {
                    evaluate(&node, cx, self);
                    self.inside_package = true;
                }
                (Statement::MetaData, _) => {}
                _ => {
                    evaluate(&node, cx, self);
                }
            }
        }
        self.append_sorted(imports, use_namespaces, variables, functions, classes);
        None
    }
}

/// `SignatureExtension`. [`SignatureExtension::new`] is `init(compilerConfig)`; `signature_directory` is only set
/// when signatures are kept.
pub struct SignatureExtension {
    pub keep_generated_signatures: bool,
    pub signature_directory: Option<String>,
}

impl SignatureExtension {
    pub fn new(keep_generated_signatures: bool, signature_directory: Option<String>) -> SignatureExtension {
        if keep_generated_signatures {
            let dir = signature_directory.unwrap_or_else(|| DEFAULT_SIG_DIR.to_string());
            SignatureExtension { keep_generated_signatures: true, signature_directory: Some(dir) }
        } else {
            SignatureExtension { keep_generated_signatures: false, signature_directory: None }
        }
    }

    /// `doSignatureGeneration(unit)`.
    fn do_signature_generation(&self, unit: &UnitRef) {
        let (cx, program, size, source) = {
            let u = unit.borrow();
            let source = u.get_source();
            let size = source.borrow().size();
            (u.context.get_asc_context(), u.get_syntax_tree().expect("NullPointerException"), size, source)
        };
        let cx = cx.or_else(|| program.borrow().program().and_then(|p| p.cx.clone())).expect("NullPointerException");
        let sig = generate_signature(&cx, &program, size, self.keep_generated_signatures);
        unit.borrow_mut().signature_checksum = compute_checksum(sig.as_deref());

        let (Some(sig), Some(dir)) = (sig, &self.signature_directory) else { return };
        let pdn = program.borrow().program().expect("ClassCastException").pkgdefs.first().cloned();
        let pkg_name = node_magic::get_package_name(pdn.as_ref()).replace('.', "_");
        let (name_for_reporting, mime_type) = {
            let s = source.borrow();
            (s.get_name_for_reporting().to_string(), s.get_mime_type())
        };
        let file_name = std::path::Path::new(&name_for_reporting).file_name().map_or_else(|| name_for_reporting.clone(), |n| n.to_string_lossy().into_owned());
        let ext = mime_mappings::get_extension(mime_type.unwrap_or_default());
        let src_name = file_name.get(..file_name.len().saturating_sub(ext.len())).unwrap_or_default();
        let sig_name = format!("{pkg_name}{}{src_name}.sig", if pkg_name.is_empty() { "" } else { "_" });
        let _ = std::fs::create_dir_all(dir);
        if std::fs::write(std::path::Path::new(dir).join(&sig_name), sig).is_err() {
            let mut warning = CompilerMessage::warning("flex2.compiler.as3.SignatureExtension$KeepGeneratedSignatureFileWritingFailed");
            warning.path = Some(name_for_reporting);
            unit.borrow_mut().context.attributes.insert(WARNING_ATTRIBUTE, Rc::new(warning));
        }
    }
}

impl Extension for SignatureExtension {
    fn parse1(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        self.do_signature_generation(unit);
    }

    fn generate(&mut self, unit: &UnitRef, _symbol_table: &mut SymbolTable) {
        if DEBUG {
            let warning = unit.borrow().context.attributes.get(WARNING_ATTRIBUTE).and_then(|w| w.downcast_ref::<CompilerMessage>().cloned());
            if let Some(warning) = warning {
                if toolkit::error_count() == 0 {
                    toolkit::log(warning, None, -1, -1, None);
                }
            }
        }
    }
}

/// `SignatureExtension.generateSignature(unit)` for a unit's context, syntax tree and `Source.size()`.
pub fn generate_signature(cx: &ContextRef, program: &NodeRef, source_size: i64, human_readable: bool) -> Option<String> {
    let buffer_size = if source_size > 0 { 2f64.powf(((source_size as f64).ln() / 2f64.ln()).round()).min(i32::MAX as f64) as usize } else { 0 };
    let mut evaluator = SignatureEvaluator::new(buffer_size, human_readable);
    evaluate(program, cx, &mut evaluator);
    Some(evaluator.get_signature())
}

/// `SignatureExtension.computeChecksum(unit, sigString)`: Adler32 over the signature's bytes.
pub fn compute_checksum(sig: Option<&str>) -> Option<i64> {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in sig?.as_bytes().chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    Some(((b << 16) | a) as i64)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use super::*;
    use crate::asc::flex2::compiler::as3::as3_compiler::{As3Compiler, SubCompiler};
    use crate::asc::flex2::compiler::configuration::Configuration;
    use crate::asc::flex2::compiler::source::{Owner, Source, VirtualFile};
    use crate::asc::util::context::ConfigVar;

    #[test]
    fn adler32_matches_java() {
        assert_eq!(compute_checksum(Some("")), Some(1));
        assert_eq!(compute_checksum(Some("Wikipedia")), Some(0x11E60398));
        assert_eq!(compute_checksum(None), None);
    }

    /// Parses each `.as` file listed in `LUMEN_SIG_FILES` as `As3Compiler.parse1` does with only this extension,
    /// writes the signature to `<LUMEN_SIG_OUT>/<n>.sig` and prints `n`, the checksum and the path. Signatures are
    /// human-readable when `LUMEN_SIG_HR` is set; `LUMEN_SIG_DEFINES` holds `NS::name=value` pairs split by `;`.
    #[test]
    fn signature_corpus() {
        let Ok(list) = std::env::var("LUMEN_SIG_FILES") else { return };
        let out = PathBuf::from(std::env::var("LUMEN_SIG_OUT").expect("LUMEN_SIG_OUT"));
        std::fs::create_dir_all(&out).unwrap();
        let human_readable = std::env::var("LUMEN_SIG_HR").is_ok();
        let mut config = Configuration::default();
        for d in std::env::var("LUMEN_SIG_DEFINES").unwrap_or_default().split(';').filter(|d| !d.is_empty()) {
            let (qname, value) = d.split_once('=').unwrap();
            let (ns, name) = qname.split_once("::").unwrap();
            config.defines.push(ConfigVar { ns: ns.to_string(), name: name.to_string(), value: value.to_string() });
        }
        let config = Rc::new(config);
        let mut report = String::new();
        for (i, path) in std::fs::read_to_string(list).unwrap().lines().enumerate() {
            let n = i + 1;
            toolkit::reset(false);
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let p = Path::new(path);
                let short_name = p.file_stem().unwrap().to_string_lossy().into_owned();
                let source = Source::new(VirtualFile::local(p), None, "", &short_name, Owner::None, false, false, true);
                let mut asc = As3Compiler::new(config.clone());
                asc.add_compiler_extension(Box::new(SignatureExtension { keep_generated_signatures: human_readable, signature_directory: None }));
                let mut symbol_table = SymbolTable::new(&config);
                let source = asc.preprocess(source)?;
                let unit = asc.parse1(&source, &mut symbol_table)?;
                let (cx, tree) = {
                    let u = unit.borrow();
                    (u.context.get_asc_context().unwrap(), u.get_syntax_tree().unwrap())
                };
                let sig = generate_signature(&cx, &tree, source.borrow().size(), human_readable).unwrap();
                std::fs::write(out.join(format!("{n}.sig")), sig).unwrap();
                let checksum = unit.borrow().signature_checksum;
                Some(checksum.map_or("null".to_string(), |c| c.to_string()))
            }));
            let result = match result {
                Ok(r) => r.unwrap_or_else(|| "null".to_string()),
                Err(e) => format!("EXC {}", e.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| e.downcast_ref::<String>().cloned()).unwrap_or_default()),
            };
            for d in toolkit::take_diagnostics() {
                eprintln!("{n}: {}", d.text);
            }
            report.push_str(&format!("{n}\t{result}\t{path}\n"));
        }
        std::fs::write(out.join("checksums.txt"), report).unwrap();
    }
}
