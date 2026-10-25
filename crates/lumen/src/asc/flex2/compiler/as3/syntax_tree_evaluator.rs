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
        SyntaxTreeEvaluator { unit, current_class_name: String::new(), seen_constructor: false, function_map: None }
    }

    fn class_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        let name = node.borrow().class_definition().unwrap().name.as_ref().and_then(|n| n.borrow().identifier().map(|i| i.name.clone())).unwrap_or_default();
        self.current_class_name = name;
        self.seen_constructor = false;
        self.function_map = Some(HashMap::new());
        walk(self, cx, node);
        self.current_class_name = String::new();
        self.seen_constructor = false;
        self.function_map = None;
    }

    fn function_definition(&mut self, cx: &ContextRef, node: &NodeRef) {
        walk(self, cx, node);
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
                MD_RESOURCEBUNDLE => self.process_resource_bundle(cx, node, pos, count),
                MD_SWF => self.unit.borrow_mut().swf_meta_data = node.borrow().as_meta_data().unwrap().get_metadata(),
                MD_FRAME => {
                    if let Some(f) = value_key("factoryClass") {
                        self.unit.borrow_mut().loader_class = Some(normalize_class_name(&f));
                    }
                    if let Some(e) = value_key("extraClass") {
                        self.unit.borrow_mut().extra_classes.insert(normalize_class_name(&e));
                    }
                }
                MD_ACCESSIBILITYCLASS => {
                    if value_key("implementation").is_none() {
                        err("[AccessibilityClass] must define an 'implementation' property specifying a type name.");
                    }
                }
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
                MD_RESOURCEBUNDLE => self.process_resource_bundle(cx, node, pos, count),
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
                            let public = attrs.as_ref().is_some_and(|a| a.borrow().as_attribute_list().is_some_and(|l| l.has_public));
                            (public, b.pos())
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

    /// `processResourceBundle`: the bundle name is recorded; mxmlc's variable-initializer rewrite is not ported.
    fn process_resource_bundle(&mut self, cx: &ContextRef, node: &NodeRef, pos: i32, count: i32) {
        if count == 0 {
            localized2(cx, Level::Error, None, pos, "ResourceBundle metadata must provide a bundle name.".to_string());
        } else {
            let name = node.borrow().as_meta_data().unwrap().get_value_index(0).unwrap_or_default();
            self.unit.borrow_mut().resource_bundle_history.insert(name);
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
