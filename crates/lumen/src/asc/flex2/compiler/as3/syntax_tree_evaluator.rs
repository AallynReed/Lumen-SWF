//! Port of `flex2.compiler.as3.SyntaxTreeEvaluator`: the parse-time metadata (`[SWF]`, `[Frame]`, `[Mixin]`, `[Event]`,
//! ...) recorded on the `CompilationUnit`, and the constructor checks. The Flex-only branches that rewrite the tree for
//! variable-level `[ResourceBundle]` are reduced to their diagnostics.

use std::collections::HashMap;

use super::evaluator_adapter::{walk, Adapter};
use crate::asc::flex2::compiler::compilation_unit::UnitRef;
use crate::asc::flex2::compiler::toolkit::{self, CompilerMessage, Level};
use crate::asc::flex2::compiler::util::{name_formatter, Name};
use crate::asc::parser::evaluator::evaluate as asc_evaluate;
use crate::asc::parser::meta_data_evaluator::MetaDataEvaluator;
use crate::asc::parser::node::{Kind, NodeRef};
use crate::asc::parser::tokens::{GET_TOKEN, SET_TOKEN};
use crate::asc::util::context::ContextRef;

const MD_RESOURCEBUNDLE: &str = "ResourceBundle";
const MD_SWF: &str = "SWF";
const MD_FRAME: &str = "Frame";
const MD_ACCESSIBILITYCLASS: &str = "AccessibilityClass";
const MD_REMOTECLASS: &str = "RemoteClass";
const MD_REQUIRESLICENSE: &str = "RequiresLicense";
const MD_MIXIN: &str = "Mixin";
const MD_ICONFILE: &str = "IconFile";
const MD_EVENT: &str = "Event";
const MD_EFFECT: &str = "Effect";
const MD_HOSTCOMPONENT: &str = "HostComponent";
const MD_SKINSTATE: &str = "SkinState";
const MD_ARRAYELEMENTTYPE: &str = "ArrayElementType";
const MD_INSTANCETYPE: &str = "InstanceType";
const MD_PERCENTPROXY: &str = "PercentProxy";
const MD_SKINPART: &str = "SkinPart";

const FLEX_METADATA: &[&str] = &[
    "AccessibilityClass",
    "ArrayElementType",
    "Bindable",
    "ChangeEvent",
    "CollapseWhiteSpace",
    "DefaultProperty",
    "Deprecated",
    "Effect",
    "Embed",
    "Event",
    "Frame",
    "HostComponent",
    "IconFile",
    "Inspectable",
    "InstanceType",
    "Managed",
    "Mixin",
    "NonCommittingChangeEvent",
    "PercentProxy",
    "RequiresLicense",
    "RemoteClass",
    "ResourceBundle",
    "Style",
    "SWF",
    "Transient",
];

/// `cx.localizedError2(origin, pos, msg)` / `localizedWarning2`.
pub fn localized2(cx: &ContextRef, level: Level, origin: Option<String>, pos: i32, text: String) {
    let pos = pos.max(0);
    let (ln, col, source, origin) = {
        let c = cx.borrow();
        let origin = origin.unwrap_or_else(|| c.get_error_origin());
        (c.get_input_line(pos), c.get_input_col(pos), c.get_error_line_text(pos), origin)
    };
    toolkit::log(CompilerMessage { level, text, path: None, line: -1, col: -1 }, Some(&origin), ln, col, Some(&source));
    if level == Level::Error {
        let statics = cx.borrow().statics.clone();
        statics.borrow_mut().err_count += 1;
    }
}

fn input_origin(cx: &ContextRef) -> Option<String> {
    cx.borrow().input.as_ref().and_then(|i| i.borrow().origin.clone())
}

pub struct SyntaxTreeEvaluator {
    unit: UnitRef,
    current_class_name: String,
    seen_constructor: bool,
    function_map: Option<HashMap<String, NodeRef>>,
    current_class_node: Option<NodeRef>,
    cur_func: Option<NodeRef>,
}

fn function_name(node: &NodeRef) -> Option<String> {
    let b = node.borrow();
    let f = b.function_definition()?;
    let name = f.name.as_ref()?.borrow();
    let id = name.as_function_name()?.identifier.as_ref()?.borrow();
    id.identifier().map(|i| i.name.clone())
}

fn function_kind(node: &NodeRef) -> i32 {
    let b = node.borrow();
    b.function_definition().and_then(|f| f.name.as_ref().and_then(|n| n.borrow().as_function_name().map(|n| n.kind))).unwrap_or(0)
}

/// `NodeMagic.getMetaData(definition)`.
fn definition_metadata(def: &NodeRef) -> Vec<NodeRef> {
    let b = def.borrow();
    let Some(md) = b.definition().and_then(|d| d.meta_data.clone()) else { return Vec::new() };
    let md = md.borrow();
    md.as_statement_list().map(|l| l.items.iter().flatten().cloned().collect()).unwrap_or_default()
}

/// `NodeMagic.retrieveClassName(n)` for a class metadata node.
fn retrieve_class_name(def: &NodeRef) -> String {
    let b = def.borrow();
    let c = b.class_definition().unwrap();
    if let Some(cframe) = &c.cframe {
        return cframe.name().to_string().replace(':', ".");
    }
    let mut s = package_name(&c.def.pkgdef);
    if !s.is_empty() {
        s.push('.');
    }
    s.push_str(&c.name.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).unwrap_or_default());
    s
}

fn package_name(pkgdef: &Option<NodeRef>) -> String {
    let Some(p) = pkgdef else { return String::new() };
    let p = p.borrow();
    let Some(name) = p.as_package_definition().and_then(|p| p.name.clone()) else { return String::new() };
    let name = name.borrow();
    let Some(id) = name.as_package_name().and_then(|n| n.id.clone()) else { return String::new() };
    let mut id = id.borrow_mut();
    id.to_identifier_string().unwrap_or_default()
}

/// `NodeMagic.normalizeClassName`.
pub fn normalize_class_name(class_name: &str) -> String {
    if !class_name.contains(':') {
        if let Some(dot) = class_name.rfind('.') {
            return format!("{}:{}", &class_name[..dot], &class_name[dot + 1..]);
        }
    }
    class_name.to_string()
}

impl SyntaxTreeEvaluator {
    pub fn new(unit: UnitRef) -> SyntaxTreeEvaluator {
        SyntaxTreeEvaluator { unit, current_class_name: String::new(), seen_constructor: false, function_map: None, current_class_node: None, cur_func: None }
    }

    fn class_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        let name = node.borrow().class_definition().unwrap().name.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).unwrap_or_default();
        self.current_class_name = name;
        self.seen_constructor = false;
        self.function_map = Some(HashMap::new());
        let prev = self.current_class_node.replace(node.clone());
        walk(self, cx, node);
        self.current_class_node = prev;
        self.current_class_name = String::new();
        self.seen_constructor = false;
        self.function_map = None;
    }

    fn function_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        let old = self.cur_func.replace(node.clone());
        walk(self, cx, node);
        self.cur_func = old;
        let name = function_name(node);
        if name.as_deref() == Some(self.current_class_name.as_str()) {
            if self.seen_constructor {
                let pos = node.borrow().pos();
                localized2(cx, Level::Error, input_origin(cx), pos, "Multiple constructor definitions found. Constructor may not be defined in <Script/> code.".to_string());
            } else {
                self.seen_constructor = true;
            }
        } else if let (Some(name), true) = (name, self.function_map.is_some()) {
            let kind = function_kind(node);
            if kind == GET_TOKEN || kind == SET_TOKEN {
                let other = self.function_map.as_ref().unwrap().get(&name).cloned();
                match other {
                    Some(other) => {
                        let (getter, setter) = if kind == GET_TOKEN { (node.clone(), other) } else { (other, node.clone()) };
                        self.process_accessor_meta_data(cx, &getter, &setter);
                    }
                    None => {
                        self.function_map.as_mut().unwrap().insert(name, node.clone());
                    }
                }
            }
        }
    }

    fn process_accessor_meta_data(&mut self, cx: &ContextRef, getter: &NodeRef, setter: &NodeRef) {
        for g in definition_metadata(getter) {
            if g.borrow().as_doc_comment().is_some() {
                continue;
            }
            for s in definition_metadata(setter) {
                if s.borrow().as_doc_comment().is_some() {
                    continue;
                }
                let (gid, gcount) = {
                    let b = g.borrow();
                    let m = b.as_meta_data().unwrap();
                    (m.get_id(), m.count())
                };
                let (sid, scount, spos) = {
                    let b = s.borrow();
                    let m = b.as_meta_data().unwrap();
                    (m.get_id(), m.count(), b.pos())
                };
                if gid == sid && sid.as_deref().is_some_and(|id| FLEX_METADATA.contains(&id)) && gcount == 0 && scount == 0 {
                    let text = format!(
                        "Duplicate '{}' metadata on {}'s '{}' getter and setter.",
                        sid.unwrap(),
                        self.current_class_name,
                        function_name(setter).unwrap_or_default()
                    );
                    localized2(cx, Level::Warning, None, spos, text);
                }
            }
        }
    }

    fn meta_data(&mut self, cx: &ContextRef, node: &NodeRef) {
        if node.borrow().as_meta_data().unwrap().data.is_some() {
            asc_evaluate(node, cx, &mut MetaDataEvaluator::new());
        }
        self.unit.borrow_mut().metadata.push(node.clone());
        let (id, def, pos) = {
            let b = node.borrow();
            let m = b.as_meta_data().unwrap();
            (m.get_id(), m.def.clone(), b.pos())
        };
        let Some(id) = id else { return };
        let value_key = |k: &str| node.borrow().as_meta_data().unwrap().get_value_key(k);
        let count = node.borrow().as_meta_data().unwrap().count();
        let value0 = || node.borrow().as_meta_data().unwrap().get_value_index(0);
        let is_class = def.as_ref().is_some_and(|d| d.borrow().class_definition().is_some());
        let err = |text: &str| localized2(cx, Level::Error, input_origin(cx), pos, text.to_string());
        if is_class {
            let def = def.unwrap();
            match id.as_str() {
                MD_RESOURCEBUNDLE => self.process_resource_bundle(cx, node, pos, count, true),
                MD_SWF => self.unit.borrow_mut().swf_meta_data = node.borrow().as_meta_data().unwrap().get_metadata(),
                MD_FRAME => {
                    if let Some(f) = value_key("factoryClass") {
                        self.unit.borrow_mut().loader_class = Some(normalize_class_name(&f));
                    }
                    if let Some(e) = value_key("extraClass") {
                        self.unit.borrow_mut().extra_classes.insert(normalize_class_name(&e));
                    }
                }
                MD_ACCESSIBILITYCLASS => match value_key("implementation") {
                    Some(i) => self.unit.borrow_mut().add_accessibility_class(i),
                    None => err("[AccessibilityClass] must define an 'implementation' property specifying a type name."),
                },
                MD_REMOTECLASS => {
                    let class_name = retrieve_class_name(&def);
                    let alias = value_key("alias").unwrap_or_else(|| format!(">{class_name}"));
                    self.unit.borrow_mut().remote_class_aliases.insert(class_name, alias);
                }
                MD_REQUIRESLICENSE => {
                    let class_name = retrieve_class_name(&def);
                    let package_name = class_name.find(':').map(|i| class_name[..i].to_string());
                    let id = value_key("id").unwrap_or_else(|| package_name.clone().unwrap_or_else(|| class_name.clone()));
                    let handler = value_key("handler").unwrap_or_else(|| match &package_name {
                        Some(p) => format!("{p}.LicenseHandler"),
                        None => format!("{id}LicenseHandler"),
                    });
                    self.unit.borrow_mut().licensed_class_reqs.insert(id, normalize_class_name(&handler));
                }
                MD_MIXIN => {
                    let c = retrieve_class_name(&def);
                    self.unit.borrow_mut().mixins.insert(c);
                }
                MD_ICONFILE => {
                    if let Some(icon) = value0() {
                        let existing = self.unit.borrow().icon.clone();
                        match existing {
                            None => self.unit.borrow_mut().icon = Some(icon),
                            Some(old) if old != icon => {
                                let src = self.unit.borrow().get_source().borrow().get_name_for_reporting().to_string();
                                err(&format!("IconFile metadata should not be set more than once in a file. Set to both '{old}' and '{icon}' within {src}."));
                            }
                            _ => {}
                        }
                    }
                }
                MD_EVENT => {
                    if let Some(t) = value_key("type") {
                        self.unit.borrow_mut().expressions.insert(Name::M(name_formatter::to_multi_name(&t)));
                    }
                }
                MD_EFFECT => {
                    let trigger = value_key("name").or_else(value0).unwrap_or_default();
                    let event = value_key("event").unwrap_or_default();
                    self.unit.borrow_mut().effect_triggers.insert(trigger, event);
                }
                MD_HOSTCOMPONENT => {
                    if count == 1 {
                        if self.unit.borrow().host_component_meta_data.is_none() {
                            let v = value0().unwrap_or_default();
                            let mut u = self.unit.borrow_mut();
                            u.expressions.insert(Name::M(name_formatter::to_multi_name(&v)));
                            u.host_component_meta_data = Some(node.clone());
                            let def = node.borrow().meta_data().and_then(|m| m.def.clone());
                            if let Some(def) = def {
                                u.host_component_owner_class = Some(super::reflect::node_magic::get_class_name(&def));
                            }
                        } else {
                            err("Only one [HostComponent] allowed.");
                        }
                    } else {
                        err("[HostComponent] must specify a type name.");
                    }
                }
                MD_SKINSTATE => {
                    if let Some(state) = value0() {
                        let added = self.unit.borrow_mut().skin_states.get_or_insert_with(Default::default).insert(state.clone());
                        if !added {
                            localized2(cx, Level::Error, None, pos, format!("Duplicate declaration of SkinState '{state}'."));
                        }
                    }
                }
                _ => {}
            }
        } else {
            match id.as_str() {
                MD_RESOURCEBUNDLE => self.process_resource_bundle(cx, node, pos, count, false),
                MD_ARRAYELEMENTTYPE => {
                    if count == 1 {
                        let v = value0().unwrap_or_default();
                        self.unit.borrow_mut().expressions.insert(Name::M(name_formatter::to_multi_name(&v)));
                    } else {
                        err("[ArrayElementType] must specify a type name.");
                    }
                }
                MD_INSTANCETYPE => {
                    if count == 1 {
                        let v = value0().unwrap_or_default();
                        self.unit.borrow_mut().expressions.insert(Name::M(name_formatter::to_multi_name(&v)));
                    } else {
                        err("[InstanceType] must specify a type name.");
                    }
                }
                MD_PERCENTPROXY => {
                    if count != 1 {
                        err("[PercentProxy] must specify a property name.");
                    }
                }
                MD_SKINPART => {
                    if let Some(def) = &def {
                        let (public, dpos) = {
                            let b = def.borrow();
                            let attrs = b.definition().and_then(|d| d.attrs.clone());
                            let has_public = attrs.as_ref().is_some_and(|a| a.borrow().as_attribute_list().is_some_and(|l| l.has_public));
                            (attrs.is_some() && has_public, b.pos())
                        };
                        let public = public || {
                            let names = super::reflect::node_magic::get_attributes(def);
                            def.borrow().definition().is_some_and(|d| d.attrs.is_some()) && (names.iter().any(|n| n == "public") || names.iter().any(|n| n.is_empty()))
                        };
                        if !public {
                            localized2(cx, Level::Error, input_origin(cx), dpos, "Skin parts must be public.".to_string());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// `processResourceBundle(context, node, onClass)`.
    fn process_resource_bundle(&mut self, cx: &ContextRef, node: &NodeRef, pos: i32, count: i32, on_class: bool) {
        if count == 0 {
            localized2(cx, Level::Error, None, pos, "ResourceBundle metadata must provide a bundle name.".to_string());
            return;
        }
        let name = node.borrow().as_meta_data().unwrap().get_value_index(0).unwrap_or_default();
        self.unit.borrow_mut().resource_bundle_history.insert(name.clone());
        if on_class {
            return;
        }
        let def = node.borrow().as_meta_data().unwrap().def.clone();
        let def_kind = def.as_ref().map(|d| match d.borrow().kind {
            Kind::VariableDefinition(_) => 0,
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => 1,
            _ => 2,
        });
        match def_kind {
            Some(0) => {
                let def = def.unwrap();
                if let Some(f) = &self.cur_func {
                    f.borrow_mut().definition_mut().unwrap().add_meta_data_node(Some(node.clone()));
                }
                let binding = {
                    let b = def.borrow();
                    let v = b.as_variable_definition().unwrap();
                    v.list.as_ref().and_then(|l| l.borrow().list().and_then(|l| l.items.first().cloned()).flatten())
                };
                let Some(binding) = binding.filter(|b| b.borrow().as_variable_binding().is_some()) else { return };
                if binding.borrow().as_variable_binding().unwrap().initializer.is_some() {
                    return;
                }
                localized2(cx, Level::Warning, input_origin(cx), pos, "Variable level ResourceBundle metadata is deprecated.  Please use class level ResourceBundle metadata.".to_string());
                let type_ = super::reflect::node_magic::lookup_type(&binding);
                if type_.as_deref() != Some("ResourceBundle") {
                    localized2(cx, Level::Error, input_origin(cx), pos, "A ResourceBundle variable must be typed to mx.resources.ResourceBundle.".to_string());
                }
                use crate::asc::parser::node::{ArgumentListNode, CallExpressionNode, GetExpressionNode, IdentifierNode, LiteralStringNode, MemberExpressionNode};
                let get_node = GetExpressionNode::new(Some(IdentifierNode::new("ResourceBundle", 0)));
                get_node.borrow_mut().set_position(0);
                let member_node = MemberExpressionNode::new(None, Some(get_node), 0);
                let lit_node = LiteralStringNode::new(&name);
                lit_node.borrow_mut().set_position(0);
                let arg_node = ArgumentListNode::new(Some(lit_node), 0);
                let ad = MemberExpressionNode::new(
                    Some(MemberExpressionNode::new(None, Some(GetExpressionNode::new(Some(IdentifierNode::new("ApplicationDomain", 0)))), 0)),
                    Some(GetExpressionNode::new(Some(IdentifierNode::new("currentDomain", 0)))),
                    0,
                );
                arg_node.borrow_mut().as_argument_list_mut().unwrap().items.push(Some(ad));
                let call_node = CallExpressionNode::new(Some(IdentifierNode::new("getResourceBundle", 0)), Some(arg_node));
                binding.borrow_mut().as_variable_binding_mut().unwrap().initializer = Some(MemberExpressionNode::new(Some(member_node), Some(call_node), 0));
                if let Some(c) = self.current_class_node.clone() {
                    let sd = crate::asc::flex2::compiler::mxml::lang::standard_defs::StandardDefs::get_standard_defs("halo");
                    super::reflect::node_magic::add_import(cx, &c, &name_formatter::to_dot(&sd.class_applicationdomain));
                }
            }
            Some(1) => {}
            _ => localized2(cx, Level::Error, None, pos, "ResourceBundle metadata is only supported on variables.".to_string()),
        }
    }
}

impl Adapter for SyntaxTreeEvaluator {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let k = match &node.borrow().kind {
            Kind::ClassDefinition(_) | Kind::BinaryClassDef(_) | Kind::BinaryInterfaceDefinition(_) => 1,
            Kind::FunctionDefinition(_) | Kind::BinaryFunctionDefinition(_) => 2,
            Kind::IncludeDirective(_) => 3,
            Kind::MetaData(_) => 4,
            _ => 0,
        };
        match k {
            1 => self.class_definition(cx, node),
            2 => self.function_definition(cx, node),
            3 => {
                let spec = node.borrow().as_include_directive().unwrap().filespec.as_ref().and_then(|f| f.borrow().as_literal_string().map(|l| l.value.clone()));
                if let Some(spec) = spec {
                    let path = crate::asc::flex2::compiler::source::canonical_path(std::path::Path::new(&spec));
                    self.unit.borrow().get_source().borrow_mut().add_file_include(path);
                }
            }
            4 => self.meta_data(cx, node),
            _ => return false,
        }
        true
    }
}

/// `getFirstClassDefinition(program)`.
pub fn get_first_class_definition(program: Option<&NodeRef>) -> Option<NodeRef> {
    let program = program?;
    let stmts = program.borrow().program()?.statements.clone()?;
    let items: Vec<NodeRef> = stmts.borrow().as_statement_list()?.items.iter().flatten().cloned().collect();
    items.into_iter().find(|n| n.borrow().class_definition().is_some())
}

/// `removeNonAPIContent(ascUnit)`: method bodies and non-const initializers go, after parsing.
pub fn remove_non_api_content(ascunit: &UnitRef) {
    if toolkit::error_count() > 0 {
        return;
    }
    let (tree, cx) = {
        let u = ascunit.borrow();
        (u.get_syntax_tree(), u.context.get_asc_context())
    };
    let Some(clsdef) = get_first_class_definition(tree.as_ref()) else { return };
    let cx = cx.expect("NullPointerException");
    let statements = clsdef.borrow().class_definition().and_then(|c| c.statements.clone());
    let items: Vec<Option<NodeRef>> = statements.as_ref().and_then(|s| s.borrow().as_statement_list().map(|l| l.items.clone())).unwrap_or_default();
    for node in items.into_iter().flatten() {
        let fexpr = node.borrow().function_definition().and_then(|f| f.fexpr.clone());
        if node.borrow().function_definition().is_some() {
            let Some(fexpr) = fexpr else { continue };
            let (body, has_result) = {
                let f = fexpr.borrow();
                let fc = f.as_function_common().expect("ClassCastException");
                let has_result = fc.signature.as_ref().is_some_and(|sig| sig.borrow().as_function_signature().is_some_and(|s| s.result.is_some()));
                (fc.body.clone(), has_result)
            };
            let Some(body) = body else { continue };
            let last = {
                let mut b = body.borrow_mut();
                match b.as_statement_list_mut() {
                    Some(l) if !l.items.is_empty() => l.items.pop().flatten(),
                    _ => continue,
                }
            };
            let nf = cx.borrow().node_factory();
            let new_body = nf.borrow_mut().statement_list(None, last);
            if has_result {
                let ret = {
                    let mut f = nf.borrow_mut();
                    let null = f.literal_null();
                    let r = f.return_statement(Some(null));
                    f.list(None, Some(r))
                };
                new_body.borrow_mut().as_statement_list_mut().unwrap().items.insert(0, Some(ret));
            }
            fexpr.borrow_mut().as_function_common_mut().unwrap().body = Some(new_body);
        } else if node.borrow().as_variable_definition().is_some() {
            let list = node.borrow().as_variable_definition().unwrap().list.clone();
            let bindings: Vec<NodeRef> = list.as_ref().and_then(|l| l.borrow().list().map(|l| l.items.iter().flatten().cloned().collect())).unwrap_or_default();
            for binding in bindings {
                let mut b = binding.borrow_mut();
                let vb = b.as_variable_binding_mut().expect("ClassCastException");
                if vb.kind != crate::asc::parser::tokens::CONST_TOKEN {
                    vb.initializer = None;
                }
            }
        }
    }
}

/// `MetaDataEvaluator`: metadata without a definition is an error.
struct MetaDataRequiresDefinitionEvaluator;

impl Adapter for MetaDataRequiresDefinitionEvaluator {
    fn visit(&mut self, cx: &ContextRef, node: &NodeRef) -> bool {
        let (is_md, def_none, pos) = {
            let b = node.borrow();
            match b.as_meta_data() {
                Some(m) => (true, m.def.is_none(), b.pos()),
                None => (false, false, 0),
            }
        };
        if !is_md {
            return false;
        }
        if def_none {
            localized2(cx, Level::Error, None, pos, "Metadata requires an associated definition.".to_string());
        }
        true
    }
}

/// `ensureMetaDataHasDefinition(compilationUnit)`.
pub fn ensure_meta_data_has_definition(unit: &UnitRef) {
    let (tree, cx) = {
        let u = unit.borrow();
        (u.get_syntax_tree(), u.context.get_asc_context())
    };
    let (Some(tree), Some(cx)) = (tree, cx) else { return };
    super::evaluator_adapter::evaluate(&mut MetaDataRequiresDefinitionEvaluator, &cx, &tree);
}
